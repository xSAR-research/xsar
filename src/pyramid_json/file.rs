//! Pyramid encoding wrapper over shared no-clobber byte publication.
use super::{JsonError, to_vec};
use crate::games::pyramid::CompleteDeal;
pub use crate::json_publication::{CleanupFailure, PublicationStatus, PublishStage};
use std::{fmt, io, path::Path};

/// Stage-aware failures, including explicitly acknowledged publication.
///
/// `Display` omits caller paths and original I/O messages. `Debug`, `source()`
/// and public fields may expose sensitive paths or platform-specific details.
/// Never interpret every `Err` as permission to retry or replace a destination.
#[derive(Debug)]
pub enum PublishError {
    /// Serialisation failed before any filesystem operation.
    Encode(JsonError),
    /// The path is empty, contains NUL, or lacks a plain final filename.
    InvalidPath,
    /// No filesystem operation is performed on non-Unix targets.
    UnsupportedPlatform,
    /// A filesystem operation failed before acknowledged publication.
    Io {
        /// Exact failed operation.
        stage: PublishStage,
        /// Whether the destination-link operation was attempted.
        status: PublicationStatus,
        /// Original filesystem failure.
        source: io::Error,
        /// Independent failure removing this call's private temporary name.
        cleanup: Option<CleanupFailure>,
    },
    /// The complete destination was published; do not replay publication blindly.
    Published {
        /// The private temporary link could not be removed, if present.
        cleanup: Option<CleanupFailure>,
        /// Parent-directory durability could not be confirmed, if present.
        directory_sync: Option<io::Error>,
    },
}


impl PublishError {
    /// Return this call's publication boundary independently of its I/O cause.
    pub fn publication_status(&self) -> PublicationStatus {
        match self {
            Self::Io { status, .. } => *status,
            Self::Published { .. } => PublicationStatus::Published,
            _ => PublicationStatus::NotPublished,
        }
    }
}


impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => write!(formatter, "Pyramid JSON not published: {error}"),
            Self::InvalidPath => {
                formatter.write_str("Pyramid JSON not published: invalid output filename")
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("Pyramid JSON publication requires Unix")
            }
            Self::Io {
                stage,
                status,
                cleanup,
                ..
            } => {
                let boundary = match status {
                    PublicationStatus::NotPublished => "not published",
                    PublicationStatus::Uncertain => "publication uncertain",
                    PublicationStatus::Published => "published",
                };
                write!(formatter, "Pyramid JSON {boundary}: failed to {stage}")?;
                if cleanup.is_some() {
                    formatter.write_str("; private temporary cleanup also failed")?;
                }
                Ok(())
            }
            Self::Published {
                cleanup,
                directory_sync,
            } => {
                formatter.write_str("Pyramid JSON published")?;
                if cleanup.is_some() {
                    formatter.write_str("; private temporary cleanup failed")?;
                }
                if directory_sync.is_some() {
                    formatter.write_str("; parent-directory durability unconfirmed")?;
                }
                Ok(())
            }
        }
    }
}


impl std::error::Error for PublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Published {
                cleanup: Some(cleanup),
                ..
            } => Some(&cleanup.source),
            Self::Published {
                directory_sync: Some(error),
                ..
            } => Some(error),
            _ => None,
        }
    }
}


/// Publish a complete deal to a new filename without overwriting any destination.
///
/// The immutable [`CompleteDeal`] has already passed model validation. Encoding
/// through [`to_vec`] finishes before any filesystem access. On Unix this opens
/// the existing parent directory, reserves a same-directory file using atomic
/// `create_new` and mode `0600` (further restricted by umask), writes all bytes,
/// calls `sync_all`, then uses `hard_link` to create the destination. An existing
/// file, directory or symlink, including a dangling symlink, is never replaced.
/// The private temporary link is then removed and the parent directory synced.
/// No rename fallback, directory creation, destination removal or overwrite exists.
///
/// `Ok(())` means linking, cleanup and both sync requests succeeded. Sync success
/// relies on OS/filesystem/device guarantees, not an absolute power-loss promise.
/// [`PublishError::Published`] means publication succeeded but cleanup and/or
/// directory durability did not. A failed hard-link call is conservatively
/// [`PublicationStatus::Uncertain`], even for `AlreadyExists`: an error on a remote
/// filesystem need not prove the server made no change. Inspect before retrying.
///
/// At most 64 predictable temporary-name candidates are tried, owning only one
/// temporary at a time. Stale files are never reclaimed. Ordinary error paths
/// report cleanup failure and its path; unwinding attempts best-effort cleanup.
/// Aborts, process death and power loss can leave a private temporary behind.
/// The byte buffer is bounded by [`super::MAX_DOCUMENT_BYTES`]; path storage is
/// proportional to the supplied path. Blocking filesystem I/O has no deadline.
///
/// The caller must select a trusted, stable parent hierarchy and must not change
/// the working directory during a relative-path call. Intermediate symlinks are
/// resolved normally. This path-based `std` implementation is not a directory-
/// capability API and cannot defend against an actor replacing ancestors or
/// the private temporary name. It only accesses the parent directory, requested
/// destination and its own temporary path; it never scans unrelated files.
///
/// Non-Unix targets return [`PublishError::UnsupportedPlatform`] without touching
/// the filesystem rather than weakening the private-file permission contract.
/// Restored/screen-recognised metadata remains a producer declaration, not proof
/// of live restoration or any authority to send input.
pub fn write_new(path: impl AsRef<Path>, deal: &CompleteDeal) -> Result<(), PublishError> {
    let bytes = to_vec(deal).map_err(PublishError::Encode)?;
    crate::json_publication::publish_bytes(path.as_ref(), &bytes).map_err(Into::into)
}

impl From<crate::json_publication::FilePublishError> for PublishError {
    fn from(error: crate::json_publication::FilePublishError) -> Self {
        use crate::json_publication::FilePublishError as E;
        match error {
            E::InvalidPath => Self::InvalidPath,
            E::UnsupportedPlatform => Self::UnsupportedPlatform,
            E::Io {
                stage,
                status,
                source,
                cleanup,
            } => Self::Io {
                stage,
                status,
                source,
                cleanup,
            },
            E::Published {
                cleanup,
                directory_sync,
            } => Self::Published {
                cleanup,
                directory_sync,
            },
        }
    }
}
