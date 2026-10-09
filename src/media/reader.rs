//! MSI media relationships and bounded payload extraction.
use super::{Error, MediaRow};
use std::io;

/// Explicit media lookup; implementations must honor the supplied byte bound.
pub trait Resolver {
    /// Return the named external cabinet or loose source file without discovery.
    fn resolve(&mut self, name: &str, max_bytes: u64) -> io::Result<Vec<u8>>;
}

/// Borrowed typed File table fields needed for media resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileSequence<'a> {
    /// MSI file identifier; this is a cabinet member, not a host path.
    pub id: &'a str,
    /// Positive File.Sequence; sparse sequences are permitted.
    pub sequence: i32,
    /// File compression override attributes.
    pub attributes: i32,
}

/// One file's media relationship, borrowing its input identifiers and references.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMedia<'a> {
    /// Borrowed file identifier.
    pub id: &'a str,
    /// File sequence.
    pub sequence: i32,
    /// Embedded/external cabinet reference, or loose source media.
    pub cabinet: Option<&'a str>,
}

fn sorted_rows(rows: &[MediaRow]) -> Result<Vec<&MediaRow>, Error> {
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by_key(|row| row.last_sequence);
    if sorted.iter().any(|row| row.last_sequence < 0)
        || sorted
            .windows(2)
            .any(|pair| pair[0].last_sequence == pair[1].last_sequence)
    {
        return Err(Error::Invalid("media sequence ordering".into()));
    }
    for row in &sorted {
        if let Some(name) = &row.cabinet {
            if name.encode_utf16().count() > 255 {
                return Err(Error::Invalid(
                    "cabinet reference exceeds Media.Cabinet schema".into(),
                ));
            }
            super::relative_name(name.strip_prefix('#').unwrap_or(name))?;
        }
    }
    Ok(sorted)
}

/// Sort and validate distinct nonnegative Media.LastSequence boundaries.
pub fn validate_media_rows(rows: &[MediaRow]) -> Result<Vec<MediaRow>, Error> {
    Ok(sorted_rows(rows)?.into_iter().cloned().collect())
}

fn reference_for<'a>(
    sequence: i32,
    attributes: i32,
    word_count: i32,
    rows: &[&'a MediaRow],
) -> Result<Option<&'a str>, Error> {
    if sequence <= 0 {
        return Err(Error::Invalid("nonpositive file sequence".into()));
    }
    if attributes & 0x6000 == 0x6000 {
        return Err(Error::Invalid(
            "contradictory file compression attributes".into(),
        ));
    }
    let row = rows
        .get(rows.partition_point(|row| row.last_sequence < sequence))
        .ok_or_else(|| Error::Invalid("missing media sequence".into()))?;
    let compressed = attributes & 0x4000 != 0
        || (attributes & 0x2000 == 0 && word_count & 2 != 0);
    if compressed {
        Ok(Some(row.cabinet.as_deref().ok_or_else(|| {
            Error::Invalid("compressed file without cabinet".into())
        })?))
    } else {
        Ok(None)
    }
}

/// Resolve compression overrides after validating every supplied media boundary.
pub fn cabinet_for(
    sequence: i32,
    attributes: i32,
    word_count: i32,
    rows: &[MediaRow],
) -> Result<Option<String>, Error> {
    reference_for(sequence, attributes, word_count, &sorted_rows(rows)?)
        .map(|name| name.map(str::to_owned))
}

/// Resolve a bounded file set into sequence order without cloning cabinet names.
/// Count, identifier/reference metadata and logical index/output scratch are
/// checked before allocating sorted indexes. This is not an allocator-wide cap.
pub fn resolve_files<'a>(
    files: &[FileSequence<'a>],
    rows: &'a [MediaRow],
    word_count: i32,
    limits: &super::Limits,
) -> Result<Vec<FileMedia<'a>>, Error> {
    if files.len() as u64 > limits.max_entries
        || rows.len() as u64 > limits.max_entries
    {
        return Err(Error::Limit("media relationship count"));
    }
    let metadata = files
        .iter()
        .map(|file| file.id.len() as u64)
        .chain(rows.iter().map(|row| {
            row.cabinet.as_ref().map_or(0, |name| name.len() as u64)
        }))
        .try_fold(0u64, u64::checked_add)
        .ok_or(Error::Limit("media relationship metadata"))?;
    if metadata > limits.max_metadata_bytes {
        return Err(Error::Limit("media relationship metadata"));
    }
    let scratch = (files.len() as u64)
        .checked_mul(
            (std::mem::size_of::<FileMedia<'_>>()
                + std::mem::size_of::<&FileSequence<'_>>()) as u64,
        )
        .and_then(|size| {
            size.checked_add(
                (rows.len() as u64)
                    .checked_mul(std::mem::size_of::<&MediaRow>() as u64)?,
            )
        })
        .ok_or(Error::Limit("media relationship scratch"))?;
    if scratch > limits.max_scratch_bytes {
        return Err(Error::Limit("media relationship scratch"));
    }
    if files.iter().any(|file| file.id.is_empty()) {
        return Err(Error::Invalid("empty file identifier".into()));
    }
    let rows = sorted_rows(rows)?;
    let mut files: Vec<_> = files.iter().collect();
    files.sort_by_key(|file| file.id);
    if files.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(Error::Invalid("duplicate file identifier".into()));
    }
    files.sort_by_key(|file| file.sequence);
    if files.windows(2).any(|pair| pair[0].sequence == pair[1].sequence) {
        return Err(Error::Invalid("duplicate file sequence".into()));
    }
    files
        .into_iter()
        .map(|file| {
            Ok(FileMedia {
                id: file.id,
                sequence: file.sequence,
                cabinet: reference_for(
                    file.sequence,
                    file.attributes,
                    word_count,
                    &rows,
                )?,
            })
        })
        .collect()
}

/// Read one declared payload. Encoded media and target bytes are independently
/// bounded by `max`; cabinet indexes are checked before codec allocation.
#[cfg(feature = "media")]
pub fn read_payload<R: io::Read + io::Seek>(
    package: &mut crate::Package<R>,
    id: &str,
    source_path: &str,
    size: u64,
    cabinet: Option<&str>,
    resolver: &mut impl Resolver,
    max: u64,
) -> Result<Vec<u8>, Error> {
    use std::io::{Cursor, Read};
    if size > max {
        return Err(Error::Limit("payload bytes"));
    }
    let bytes = if let Some(name) = cabinet {
        let data = if let Some(stream) = name.strip_prefix('#') {
            let mut bytes = Vec::new();
            package
                .read_stream(stream)?
                .take(max.saturating_add(1))
                .read_to_end(&mut bytes)?;
            bytes
        } else {
            resolver.resolve(name, max)?
        };
        if data.len() as u64 > max {
            return Err(Error::Limit("media bytes"));
        }
        preflight(&data)?;
        let mut cabinet = cabinet::Cabinet::new(Cursor::new(data))
            .map_err(Error::Cabinet)?;
        let member = cabinet
            .entry(id)
            .ok_or_else(|| Error::Integrity(format!("cabinet member {id}")))?;
        if u64::from(member.size) != size {
            return Err(Error::Integrity(format!("size of {id}")));
        }
        let limit = usize::try_from(size)
            .map_err(|_| Error::Limit("payload address space"))?;
        cabinet.read_file_bytes(id, limit).map_err(Error::Cabinet)?
    } else {
        resolver.resolve(source_path, size)?
    };
    if bytes.len() as u64 != size {
        return Err(Error::Integrity(format!("size of {id}")));
    }
    Ok(bytes)
}

#[cfg(feature = "media")]
fn preflight(data: &[u8]) -> Result<(), Error> {
    let invalid = || Error::Invalid("cabinet index bounds".into());
    if data.len() < 36 || &data[..4] != b"MSCF" {
        return Err(invalid());
    }
    let u16_at = |offset| u16::from_le_bytes([data[offset], data[offset + 1]]);
    let u32_at = |offset| {
        u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    };
    let size = usize::try_from(u32_at(8)).map_err(|_| invalid())?;
    if size < 36 || size > data.len() {
        return Err(invalid());
    }
    let flags = u16_at(30);
    if flags & 3 != 0 {
        return Err(Error::Unsupported("spanning cabinets".into()));
    }
    if flags & !7 != 0 {
        return Err(invalid());
    }
    let (folder_start, folder_reserved, data_reserved) = if flags & 4 != 0 {
        if size < 40 {
            return Err(invalid());
        }
        let start = 40usize
            .checked_add(usize::from(u16_at(36)))
            .filter(|start| *start <= size)
            .ok_or_else(invalid)?;
        (start, usize::from(data[38]), usize::from(data[39]))
    } else {
        (36, 0, 0)
    };
    let folders = usize::from(u16_at(26));
    let files = usize::from(u16_at(28));
    let folder_stride = 8 + folder_reserved;
    let folder_end = folder_start
        .checked_add(folders.checked_mul(folder_stride).ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    let mut position = usize::try_from(u32_at(16)).map_err(|_| invalid())?;
    if position < folder_end || position > size || folder_end > size {
        return Err(invalid());
    }
    for _ in 0..files {
        let end = position
            .checked_add(16)
            .filter(|end| *end <= size)
            .ok_or_else(invalid)?;
        let folder = u16_at(position + 8);
        if folder >= 0xfffd {
            return Err(Error::Unsupported("spanning cabinet members".into()));
        }
        if usize::from(folder) >= folders {
            return Err(invalid());
        }
        let name_len = data[end..size]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(invalid)?;
        position = end.checked_add(name_len + 1).ok_or_else(invalid)?;
    }
    for folder in 0..folders {
        let offset = folder_start + folder * folder_stride;
        let mut block =
            usize::try_from(u32_at(offset)).map_err(|_| invalid())?;
        for _ in 0..u16_at(offset + 4) {
            let end = block
                .checked_add(8 + data_reserved)
                .filter(|end| *end <= size)
                .ok_or_else(invalid)?;
            let compressed = usize::from(u16_at(block + 4));
            if u16_at(block + 6) == 0 {
                return Err(Error::Unsupported(
                    "spanning cabinet data blocks".into(),
                ));
            }
            block = end
                .checked_add(compressed)
                .filter(|end| *end <= size)
                .ok_or_else(invalid)?;
        }
    }
    Ok(())
}
