//! Bounded stored-cabinet preparation and explicit media emission.
//! Adapted from ms-package installer_media.rs; see licenses/ms-package/LICENSE.
use super::{
    Artifact, CabinetSpec, Error, Layout, Limits, PreparedMedia, Report, Sink,
    relative_name,
};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
};
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn leaf(name: &str) -> Result<(), Error> {
    relative_name(name)
}

fn reject_path_collisions(names: &BTreeSet<String>) -> Result<(), Error> {
    for name in names {
        for (index, _) in name.match_indices('/') {
            if names.contains(&name[..index]) {
                return Err(invalid(
                    "MSI media artifact file/directory collision",
                ));
            }
        }
    }
    Ok(())
}
/// Prepare complete bounded media from ordered (File identifier, source name, bytes).
/// This does not modify an MSI database or open caller destinations.
pub fn prepare(
    files: &[(&str, &str, &[u8])],
    layout: &Layout,
    directory_name: &str,
    limits: &Limits,
) -> Result<PreparedMedia, Error> {
    if files.is_empty() || files.len() > i32::MAX as usize {
        return Err(invalid(
            "MSI media requires positive representable file sequences",
        ));
    }
    if files.len() as u64 > limits.max_entries {
        return Err(Error::Limit("MSI media files"));
    }
    relative_name(directory_name)?;
    let mut names = BTreeSet::new();
    let mut identifiers = BTreeSet::new();
    let mut source_bytes = 0u64;
    let mut metadata = directory_name.len() as u64;
    for (id, name, bytes) in files {
        // File identifiers are database keys, not Windows destination names.
        // The cabinet codec separately checks its member-name representation.
        if id.is_empty() || id.contains('\0') {
            return Err(invalid("empty or invalid MSI file identifier"));
        }
        leaf(name)?;
        if !names.insert(name.to_lowercase())
            || !identifiers.insert(id.to_lowercase())
        {
            return Err(invalid(
                "duplicate MSI media file names or identifiers",
            ));
        }
        if bytes.len() as u64 > limits.max_file_bytes {
            return Err(Error::Limit("MSI media payload"));
        }
        source_bytes = source_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit("MSI media bytes"))?;
        metadata = metadata
            .checked_add(id.len() as u64)
            .and_then(|n| n.checked_add(name.len() as u64))
            .ok_or(Error::Limit("MSI media metadata"))?;
    }
    reject_path_collisions(&names)?;
    reject_path_collisions(&identifiers)?;
    if source_bytes > limits.max_total_bytes
        || source_bytes > limits.max_scratch_bytes
        || metadata > limits.max_metadata_bytes
    {
        return Err(Error::Limit("MSI media source resources"));
    }
    let mut prepared = PreparedMedia {
        rows: Vec::new(),
        embedded: Vec::new(),
        external: Vec::new(),
        compressed: !matches!(layout, Layout::Loose),
    };
    if matches!(layout, Layout::Loose) {
        if source_bytes
            .checked_mul(2)
            .is_none_or(|n| n > limits.max_scratch_bytes)
            || source_bytes > limits.max_output_bytes
        {
            return Err(Error::Limit("loose MSI media scratch/output"));
        }
        for (_, name, bytes) in files {
            prepared
                .external
                .push((format!("{directory_name}/{name}"), bytes.to_vec()));
        }
        prepared.rows.push((files.len() as i32, None));
        return Ok(prepared);
    }
    let specs: Cow<'_, [CabinetSpec]> = match layout {
        Layout::Embedded => Cow::Owned(vec![CabinetSpec {
            name: "payload.cab".into(),
            file_count: files.len() as u64,
            embedded: true,
        }]),
        Layout::ExternalCabinet { name } => {
            leaf(name)?;
            Cow::Owned(vec![CabinetSpec {
                name: name.clone(),
                file_count: files.len() as u64,
                embedded: false,
            }])
        }
        Layout::Cabinets { cabinets } => Cow::Borrowed(cabinets),
        Layout::Loose => return Err(invalid("unexpected cabinet layout")),
    };
    if specs.is_empty()
        || specs.len() > 32767
        || specs.len() as u64 > limits.max_entries
    {
        return Err(Error::Limit("MSI cabinet count"));
    }
    let mut media_names = BTreeSet::new();
    let mut file_count = 0u64;
    for spec in specs.iter() {
        leaf(&spec.name)?;
        if spec.name.encode_utf16().count() + usize::from(spec.embedded) > 255
        {
            return Err(invalid(
                "MSI Media.Cabinet reference exceeds 255 UTF-16 units",
            ));
        }
        if !spec.name.to_lowercase().ends_with(".cab")
            || spec.name.starts_with('#')
            || spec.file_count == 0
            || !media_names.insert(spec.name.to_lowercase())
        {
            return Err(invalid(
                "invalid or duplicate MSI cabinet specification",
            ));
        }
        if spec.embedded
            && !crate::internal::streamname::is_valid(&spec.name, false)
        {
            return Err(invalid("invalid embedded cabinet stream name"));
        }
        file_count = file_count
            .checked_add(spec.file_count)
            .ok_or(Error::Limit("MSI cabinet files"))?;
        metadata = metadata
            .checked_add(spec.name.len() as u64)
            .ok_or(Error::Limit("MSI media metadata"))?;
    }
    reject_path_collisions(&media_names)?;
    if file_count != files.len() as u64 {
        return Err(invalid(
            "cabinet groups must cover every file exactly once",
        ));
    }
    if metadata > limits.max_metadata_bytes {
        return Err(Error::Limit("MSI media metadata"));
    }
    let mut index = 0usize;
    let mut staged = 0u64;
    for spec in specs.iter() {
        let end = index
            .checked_add(
                usize::try_from(spec.file_count)
                    .map_err(|_| Error::Limit("MSI cabinet files"))?,
            )
            .ok_or(Error::Limit("MSI cabinet files"))?;
        let mut cabinet =
            cabinet::CabinetBuilder::new(cabinet::WriteCompression::None);
        for (id, _, bytes) in &files[index..end] {
            cabinet
                .add_file(id, bytes)
                .map_err(|error| Error::Invalid(error.to_string()))?;
        }
        let available = limits
            .max_scratch_bytes
            .checked_sub(source_bytes)
            .and_then(|n| n.checked_sub(staged))
            .and_then(|n| n.checked_sub(32768))
            .ok_or(Error::Limit("MSI cabinet scratch"))?;
        let mut output = BoundedCursor {
            inner: Cursor::new(Vec::new()),
            limit_hit: false,
            limit: available
                .min(limits.max_file_bytes)
                .min(limits.max_output_bytes.saturating_sub(staged)),
        };
        if let Err(error) = cabinet.write(&mut output) {
            return Err(if output.limit_hit {
                Error::Limit("MSI cabinet output/scratch")
            } else {
                Error::Io(io::Error::other(error.to_string()))
            });
        }
        output.flush()?;
        let bytes = output.inner.into_inner();
        staged = staged
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit("MSI media output"))?;
        prepared.rows.push((
            end as i32,
            Some(if spec.embedded {
                format!("#{}", spec.name)
            } else {
                spec.name.clone()
            }),
        ));
        if spec.embedded {
            prepared.embedded.push((spec.name.clone(), bytes));
        } else {
            prepared.external.push((spec.name.clone(), bytes));
        }
        index = end;
    }
    Ok(prepared)
}

/// Finalize explicitly named media artifacts in caller-provided destinations.
/// Preflight rejects conflicting names and budgets before opening any sink.
/// `Error::Emission` records completed artifacts and the partial current name.
pub fn write_media<S: Sink>(
    artifacts: &[(String, Vec<u8>)],
    sink: &mut S,
    limits: &Limits,
) -> Result<Report, Error> {
    if artifacts.len() as u64 > limits.max_entries {
        return Err(Error::Limit("MSI media artifacts"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    let mut metadata = 0u64;
    for (name, bytes) in artifacts {
        relative_name(name)?;
        if !names.insert(name.to_lowercase()) {
            return Err(invalid("duplicate MSI media artifact name"));
        }
        if bytes.len() as u64 > limits.max_file_bytes {
            return Err(Error::Limit("MSI media artifact bytes"));
        }
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit("MSI media output"))?;
        metadata = metadata
            .checked_add(name.len() as u64)
            .ok_or(Error::Limit("MSI media names"))?;
    }
    for name in &names {
        for (index, _) in name.match_indices('/') {
            if names.contains(&name[..index]) {
                return Err(invalid(
                    "MSI media artifact file/directory collision",
                ));
            }
        }
    }
    if total > limits.max_output_bytes
        || total > limits.max_scratch_bytes
        || metadata > limits.max_metadata_bytes
    {
        return Err(Error::Limit("MSI media emission resources"));
    }
    let mut report = Report::default();
    for (name, bytes) in artifacts {
        let mut written = 0u64;
        let result = (|| -> Result<(), Error> {
            let mut output = sink.create(name)?;
            while (written as usize) < bytes.len() {
                match output.write(&bytes[written as usize..]) {
                    Ok(0) => {
                        return Err(
                            io::Error::from(io::ErrorKind::WriteZero).into()
                        );
                    }
                    Ok(count) if count <= bytes.len() - written as usize => {
                        written += count as u64
                    }
                    Ok(_) => {
                        return Err(io::Error::other(
                            "media sink returned an invalid write count",
                        )
                        .into());
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::Interrupted =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            output.flush()?;
            sink.finish(name, output)?;
            Ok(())
        })();
        report.bytes_written += written;
        if let Err(source) = result {
            return Err(Error::Emission {
                completed: report
                    .completed
                    .iter()
                    .map(|artifact| artifact.name.clone())
                    .collect(),
                incomplete: name.clone(),
                bytes_written: report.bytes_written,
                source: Box::new(source),
            });
        }
        report.completed.push(Artifact { name: name.clone(), bytes: written });
    }
    Ok(report)
}

struct BoundedCursor {
    inner: Cursor<Vec<u8>>,
    limit: u64,
    limit_hit: bool,
}
impl Read for BoundedCursor {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}
impl Seek for BoundedCursor {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}
impl Write for BoundedCursor {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .inner
            .position()
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > self.limit)
        {
            self.limit_hit = true;
            return Err(io::Error::other(
                "media output/scratch byte limit exceeded",
            ));
        }
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
