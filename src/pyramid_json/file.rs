//! Private implementation of additive, no-clobber JSON filesystem publication.

use super::{to_vec, JsonError};
use crate::games::pyramid::CompleteDeal;
use std::{
    fmt, io,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    sync::atomic::{AtomicUsize, Ordering},
};


/// What this call knows about publication, not whether an older destination exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationStatus {
    /// No destination-link operation was attempted by this call.
    NotPublished,
    /// The link operation failed; inspect the destination before any retry.
    ///
    /// Even an `AlreadyExists` error is classified conservatively: some remote
    /// filesystems can report an error after a server-side operation completed.
    Uncertain,
    /// The link operation succeeded, but cleanup or directory sync failed.
    Published,
}


/// The failed operation before a successful destination link was acknowledged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishStage {
    /// Open the existing parent directory for its eventual metadata sync.
    OpenDirectory,
    /// Reserve one private temporary file, with at most 64 candidate attempts.
    CreateTemporary,
    /// Write the complete serialised document to the private file.
    WriteTemporary,
    /// Sync the private file's data and metadata before making it visible.
    SyncTemporary,
    /// Atomically create the destination hard link without replacing anything.
    LinkDestination,
}


impl fmt::Display for PublishStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OpenDirectory => "open parent directory",
            Self::CreateTemporary => "create private temporary file",
            Self::WriteTemporary => "write private temporary file",
            Self::SyncTemporary => "sync private temporary file",
            Self::LinkDestination => "link destination without replacement",
        })
    }
}


/// A private temporary name whose removal could not be confirmed.
///
/// The name is retained for operator inspection, not automatically retried.
/// Do not remove a path later without verifying that it still names this file.
#[derive(Debug)]
pub struct CleanupFailure {
    /// Exact attempted temporary path; it may no longer exist after external changes.
    pub path: PathBuf,
    /// Original unlink error, potentially containing caller-sensitive text.
    pub source: io::Error,
}


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
    #[cfg(unix)]
    {
        publish_bytes(path.as_ref(), &bytes)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        Err(PublishError::UnsupportedPlatform)
    }
}


/// Per-process uniqueness without randomness or counter wrap.
#[cfg(unix)]
static TEMPORARY_SEQUENCE: AtomicUsize = AtomicUsize::new(0);


/// Maximum candidate reservations, including skipped destination-name collisions.
#[cfg(unix)]
const TEMPORARY_ATTEMPTS: usize = 64;


/// Own exactly one created name until explicit cleanup or unwinding.
#[cfg(unix)]
struct TemporaryFile {
    /// `None` after a cleanup attempt, including an explicitly reported failure.
    path: Option<PathBuf>,
}


#[cfg(unix)]
impl TemporaryFile {
    /// Reserve a new file, never opening or removing another call's candidate.
    fn create(destination: &Path, parent: &Path) -> io::Result<(Self, File)> {
        Self::create_with(destination, parent, || {
            TEMPORARY_SEQUENCE
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |sequence| {
                    sequence.checked_add(1)
                })
                .map(|sequence| format!(".xsar-pyramid-{}-{sequence}.tmp", std::process::id()))
                .map_err(|_| io::Error::other("private temporary sequence exhausted"))
        })
    }


    /// Keep bounded reservation independently testable with colliding candidates.
    fn create_with(
        destination: &Path,
        parent: &Path,
        mut candidate_name: impl FnMut() -> io::Result<String>,
    ) -> io::Result<(Self, File)> {
        for _ in 0..TEMPORARY_ATTEMPTS {
            let path = parent.join(candidate_name()?);
            if path.file_name() == destination.file_name() {
                continue;
            }
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => return Ok((Self { path: Some(path) }, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "private temporary candidates exhausted",
        ))
    }


    /// Attempt unlink once and retain failure evidence rather than retrying in Drop.
    fn cleanup(&mut self) -> Option<CleanupFailure> {
        let path = self.path.take()?;
        fs::remove_file(&path)
            .err()
            .map(|source| CleanupFailure { path, source })
    }
}


#[cfg(unix)]
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}


/// Run the publication sequence only after complete serialisation.
#[cfg(unix)]
fn publish_bytes(destination: &Path, bytes: &[u8]) -> Result<(), PublishError> {
    let parent = checked_parent(destination)?;
    let directory = File::open(parent.join(".")).map_err(|source| PublishError::Io {
        stage: PublishStage::OpenDirectory,
        status: PublicationStatus::NotPublished,
        source,
        cleanup: None,
    })?;
    let (mut temporary, file) =
        TemporaryFile::create(destination, parent).map_err(|source| PublishError::Io {
            stage: PublishStage::CreateTemporary,
            status: PublicationStatus::NotPublished,
            source,
            cleanup: None,
        })?;
    if let Err((stage, source)) = write_and_sync(file, bytes) {
        return Err(operation_failure(&mut temporary, stage, source));
    }
    let temporary_path = temporary
        .path
        .as_ref()
        .expect("created temporary owns its path");
    if let Err(source) = fs::hard_link(temporary_path, destination) {
        return Err(operation_failure(
            &mut temporary,
            PublishStage::LinkDestination,
            source,
        ));
    }
    let cleanup = temporary.cleanup();
    publication_result(cleanup, directory.sync_all())
}


/// Reject raw trailing slash/dot components before Path's normalisation hides them.
#[cfg(unix)]
fn checked_parent(destination: &Path) -> Result<&Path, PublishError> {
    let bytes = destination.as_os_str().as_bytes();
    let filename = bytes
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    if filename.is_empty() || filename == b"." || filename == b".." || bytes.contains(&0) {
        return Err(PublishError::InvalidPath);
    }
    Ok(destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new(".")))
}


/// Consume the handle so it is closed before unlink on every ordinary return.
#[cfg(unix)]
fn write_and_sync(mut file: File, bytes: &[u8]) -> Result<(), (PublishStage, io::Error)> {
    file.write_all(bytes)
        .map_err(|error| (PublishStage::WriteTemporary, error))?;
    file.sync_all()
        .map_err(|error| (PublishStage::SyncTemporary, error))
}


/// Preserve primary failure and the independent cleanup failure, if any.
#[cfg(unix)]
fn operation_failure(
    temporary: &mut TemporaryFile,
    stage: PublishStage,
    source: io::Error,
) -> PublishError {
    PublishError::Io {
        stage,
        status: if stage == PublishStage::LinkDestination {
            PublicationStatus::Uncertain
        } else {
            PublicationStatus::NotPublished
        },
        source,
        cleanup: temporary.cleanup(),
    }
}


/// Report both post-publication failures without discarding an acknowledged link.
#[cfg(unix)]
fn publication_result(
    cleanup: Option<CleanupFailure>,
    sync: io::Result<()>,
) -> Result<(), PublishError> {
    let directory_sync = sync.err();
    if cleanup.is_some() || directory_sync.is_some() {
        Err(PublishError::Published {
            cleanup,
            directory_sync,
        })
    } else {
        Ok(())
    }
}


#[cfg(all(test, unix))]
mod tests;
