//! Explicit MSI media planning and caller-controlled I/O.
//! Pure contracts are available without features. Cabinet operations require `media`.
//! Contracts adapted from ms-package; its preserved MIT notice is in
//! `licenses/ms-package/LICENSE`.
use std::{fmt, io};

mod reader;
pub use reader::*;
#[cfg(feature = "media")]
mod writer;
#[cfg(feature = "media")]
pub use writer::{prepare, write_media};

/// Resource bounds for logical buffers and actual media I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum files or media artifacts.
    pub max_entries: u64,
    /// Maximum retained metadata bytes.
    pub max_metadata_bytes: u64,
    /// Maximum one payload or artifact size.
    pub max_file_bytes: u64,
    /// Maximum total decoded payload bytes.
    pub max_total_bytes: u64,
    /// Maximum aggregate emitted media bytes.
    pub max_output_bytes: u64,
    /// Maximum logical retained payload, media and codec scratch bytes.
    pub max_scratch_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            max_metadata_bytes: 16 << 20,
            max_file_bytes: 256 << 20,
            max_total_bytes: 512 << 20,
            max_output_bytes: 640 << 20,
            max_scratch_bytes: 640 << 20,
        }
    }
}

/// Failure of bounded media planning, resolution or emission.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Underlying caller or storage I/O failure.
    Io(io::Error),
    /// Cabinet parser or payload codec I/O failure.
    Cabinet(io::Error),
    /// A named resource budget was exceeded.
    Limit(&'static str),
    /// Invalid media representation or request.
    Invalid(String),
    /// Resolved bytes disagree with package metadata.
    Integrity(String),
    /// A representation is outside the supported media contract.
    Unsupported(String),
    /// A media destination failed after zero or more artifacts completed.
    Emission {
        /// Successfully finalized artifact names in order.
        completed: Vec<String>,
        /// Current incomplete artifact name.
        incomplete: String,
        /// Actual aggregate bytes written, including the partial artifact.
        bytes_written: u64,
        /// Underlying failure.
        source: Box<Error>,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) | Self::Cabinet(error) => write!(f, "{error}"),
            Self::Limit(resource) => {
                write!(f, "media limit exceeded: {resource}")
            }
            Self::Invalid(message) => write!(f, "invalid media: {message}"),
            Self::Integrity(message) => {
                write!(f, "media integrity: {message}")
            }
            Self::Unsupported(message) => {
                write!(f, "unsupported media: {message}")
            }
            Self::Emission { incomplete, bytes_written, source, .. } => {
                write!(
                    f,
                    "media emission failed at {incomplete} after {bytes_written} bytes: {source}"
                )
            }
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::Cabinet(error) => Some(error),
            Self::Emission { source, .. } => Some(source),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
/// Result of a media operation.
pub type Result<T> = std::result::Result<T, Error>;

/// A contiguous group of file sequences stored in one independent cabinet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabinetSpec {
    /// Portable cabinet name, without the embedded-stream `#` marker.
    pub name: String,
    /// Number of successive files assigned to this cabinet; must be nonzero.
    pub file_count: u64,
    /// Embed the cabinet in the MSI; otherwise publish it through the media sink.
    pub embedded: bool,
}

/// Explicit source media for an ordered list of files.
/// Cabinet groups follow file insertion order and never span cabinets.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Layout {
    /// One embedded stored `payload.cab` (the existing default).
    #[default]
    Embedded,
    /// One external stored cabinet, resolved by its explicit name when reading.
    ExternalCabinet {
        /// Portable relative `.cab` name.
        name: String,
    },
    /// Loose source files under `directory_name/target_filename`.
    Loose,
    /// Explicit successive embedded and/or external, nonspanning cabinets.
    Cabinets {
        /// Ordered groups covering every file exactly once.
        cabinets: Vec<CabinetSpec>,
    },
}

/// Caller-controlled destinations for external cabinets and loose files.
///
/// The library opens each explicit name, writes bounded bytes, and flushes the
/// writer before calling `finish`. Implement `finish` to finalize any additional
/// storage operation and propagate its failures. Atomic publication and cleanup
/// of incomplete artifacts are the caller's responsibility.
pub trait Sink {
    /// Writer for one explicitly named artifact.
    type Writer: io::Write;
    /// Open a separate destination for the supplied portable relative name.
    fn create(&mut self, name: &str) -> io::Result<Self::Writer>;
    /// Finalize the flushed writer; success records this artifact as completed.
    fn finish(&mut self, name: &str, writer: Self::Writer) -> io::Result<()>;
}

/// A fully finalized external artifact, without a durability or trust claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    /// Explicit relative media name supplied to the sink.
    pub name: String,
    /// Actual artifact bytes written and finalized.
    pub bytes: u64,
}

/// External artifacts completed by a successful emission operation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    /// Artifacts in emission order, after successful flush and sink finalization.
    pub completed: Vec<Artifact>,
    /// Aggregate actual output bytes.
    pub bytes_written: u64,
}

/// Immutable, validated media bytes and MSI Media table values.
/// Cabinet groups never span cabinets. No filesystem destinations are opened.
#[derive(Debug)]
pub struct PreparedMedia {
    rows: Vec<(i32, Option<String>)>,
    embedded: Vec<(String, Vec<u8>)>,
    external: Vec<(String, Vec<u8>)>,
    compressed: bool,
}
impl PreparedMedia {
    /// Ordered LastSequence and Cabinet values.
    pub fn rows(&self) -> &[(i32, Option<String>)] {
        &self.rows
    }
    /// Embedded cabinet stream names and complete bytes.
    pub fn embedded(&self) -> &[(String, Vec<u8>)] {
        &self.embedded
    }
    /// External cabinet or loose-file names and complete bytes.
    pub fn external(&self) -> &[(String, Vec<u8>)] {
        &self.external
    }
    /// Whether cabinet-compressed media flags are required.
    pub fn compressed(&self) -> bool {
        self.compressed
    }
    /// Consume the plan without cloning its complete media buffers.
    pub fn into_parts(self) -> PreparedParts {
        (self.rows, self.embedded, self.external, self.compressed)
    }

    /// Insert complete embedded cabinet streams into an explicitly supplied MSI.
    /// Existing streams are rejected before mutation. An I/O failure can leave
    /// partial database changes; this operation does not provide atomicity.
    #[cfg(feature = "media")]
    pub fn write_embedded<R: io::Read + io::Write + io::Seek>(
        &self,
        package: &mut crate::Package<R>,
    ) -> Result<()> {
        use io::Write;
        for (name, _) in &self.embedded {
            if package.has_stream(name) {
                return Err(Error::Invalid(format!(
                    "embedded stream already exists: {name}"
                )));
            }
            if !crate::internal::streamname::is_valid(name, false) {
                return Err(Error::Invalid(format!(
                    "invalid embedded stream name: {name}"
                )));
            }
        }
        for (name, bytes) in &self.embedded {
            let mut stream = package.write_stream(name)?;
            stream.write_all(bytes)?;
            stream.flush()?;
        }
        Ok(())
    }
}

/// Update only the MSI SummaryInfo compressed-media flag, preserving other bits.
pub fn summary_word_count(word_count: i32, compressed: bool) -> i32 {
    if compressed { word_count | 2 } else { word_count & !2 }
}

/// Owned rows, embedded media, external media and compressed-media flag.
pub type PreparedParts = (
    Vec<(i32, Option<String>)>,
    Vec<(String, Vec<u8>)>,
    Vec<(String, Vec<u8>)>,
    bool,
);

pub(crate) fn relative_name(name: &str) -> Result<()> {
    for part in name.split('/') {
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with(['.', ' '])
            || part.chars().any(|c| c.is_control() || "\\:*?\"<>|".contains(c))
        {
            return Err(Error::Invalid("unsafe relative media name".into()));
        }
        let stem = part.split('.').next().unwrap_or_default().to_uppercase();
        let suffix =
            stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT"));
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || suffix.is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "¹"
                        | "²"
                        | "³"
                )
            })
        {
            return Err(Error::Invalid("reserved Windows media name".into()));
        }
    }
    Ok(())
}

/// One ordered MSI Media boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRow {
    /// Inclusive last File sequence represented by this row.
    pub last_sequence: i32,
    /// Embedded `#stream`, external cabinet name, or loose media.
    pub cabinet: Option<String>,
}
