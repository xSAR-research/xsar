//! Private failure-boundary tests alongside the real public filesystem tests.

use super::*;
use std::{
    error::Error,
    os::unix::fs::{symlink, DirBuilderExt},
};


/// Distinct directory namespace from the integration suite and temporary files.
static DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(0);


/// Exclusive test-owned directory; never reuses a stale name.
struct TestDirectory(PathBuf);


impl TestDirectory {
    /// Reserve a private directory beneath the isolated crate checkout.
    fn new() -> Self {
        let sequence = DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!(".publisher-unit-{}-{sequence}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}


impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}


#[test]
fn candidate_exhaustion_is_bounded_and_preserves_existing_bytes() {
    let directory = TestDirectory::new();
    let occupied = directory.0.join("occupied");
    fs::write(&occupied, b"other owner's bytes").unwrap();
    let mut attempts = 0;
    let result = TemporaryFile::create_with(&directory.0.join("deal.json"), &directory.0, || {
        attempts += 1;
        Ok("occupied".to_owned())
    });
    assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::AlreadyExists));
    assert_eq!(attempts, TEMPORARY_ATTEMPTS);
    assert_eq!(fs::read(occupied).unwrap(), b"other owner's bytes");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}


#[test]
fn destination_name_cannot_be_reserved_as_the_temporary() {
    let directory = TestDirectory::new();
    let mut attempts = 0;
    let result = TemporaryFile::create_with(&directory.0.join("deal.json"), &directory.0, || {
        attempts += 1;
        Ok("deal.json".to_owned())
    });
    assert!(result.is_err());
    assert_eq!(attempts, TEMPORARY_ATTEMPTS);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}


#[test]
fn occupied_symlink_candidate_is_not_followed_and_next_candidate_succeeds() {
    let directory = TestDirectory::new();
    let absent = directory.0.join("absent");
    let occupied = directory.0.join("occupied");
    symlink(&absent, &occupied).unwrap();
    let mut attempts = 0;
    let (mut temporary, file) =
        TemporaryFile::create_with(&directory.0.join("deal.json"), &directory.0, || {
            attempts += 1;
            Ok(if attempts == 1 { "occupied" } else { "free" }.to_owned())
        })
        .unwrap();
    assert_eq!(attempts, 2);
    assert!(!absent.exists());
    assert_eq!(fs::read_link(&occupied).unwrap(), absent);
    drop(file);
    assert!(temporary.cleanup().is_none());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}


#[test]
fn unwinding_removes_only_the_owned_temporary() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("deal.json");
    fs::write(&destination, b"existing destination").unwrap();
    let result = std::panic::catch_unwind(|| {
        let (_temporary, _file) = TemporaryFile::create(&destination, &directory.0).unwrap();
        panic!("injected unwind after reservation");
    });
    assert!(result.is_err());
    assert_eq!(fs::read(destination).unwrap(), b"existing destination");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}


#[test]
fn read_only_handle_write_failure_retains_stage_and_cleans_temporary() {
    let directory = TestDirectory::new();
    let (mut temporary, file) =
        TemporaryFile::create(&directory.0.join("deal.json"), &directory.0).unwrap();
    drop(file);
    let reader = File::open(temporary.path.as_ref().unwrap()).unwrap();
    let (stage, source) = write_and_sync(reader, b"cannot write this handle").unwrap_err();
    assert_eq!(stage, PublishStage::WriteTemporary);
    let error = operation_failure(&mut temporary, stage, source);
    assert_eq!(error.publication_status(), PublicationStatus::NotPublished);
    assert!(matches!(error, PublishError::Io { cleanup: None, .. }));
    assert!(error.source().is_some());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}


#[test]
fn sync_failure_is_not_published_and_removes_the_owned_temporary() {
    let directory = TestDirectory::new();
    let (mut temporary, file) =
        TemporaryFile::create(&directory.0.join("deal.json"), &directory.0).unwrap();
    drop(file);
    let error = operation_failure(
        &mut temporary,
        PublishStage::SyncTemporary,
        io::Error::other("injected sync failure"),
    );
    assert_eq!(error.publication_status(), PublicationStatus::NotPublished);
    assert!(matches!(
        error,
        PublishError::Io {
            stage: PublishStage::SyncTemporary,
            cleanup: None,
            ..
        }
    ));
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}


#[test]
fn failed_cleanup_is_retained_and_not_silently_retried_on_drop() {
    let directory = TestDirectory::new();
    let retained = directory.0.join("injected-unlink-failure");
    fs::create_dir(&retained).unwrap();
    let mut temporary = TemporaryFile {
        path: Some(retained.clone()),
    };
    let failure = temporary.cleanup().unwrap();
    assert_eq!(failure.path, retained);
    assert!(temporary.path.is_none());
    fs::remove_dir(&retained).unwrap();
    fs::write(&retained, b"replacement must survive Drop").unwrap();
    drop(temporary);
    assert_eq!(
        fs::read(retained).unwrap(),
        b"replacement must survive Drop"
    );
}


#[test]
fn primary_error_and_cleanup_error_are_both_preserved() {
    let directory = TestDirectory::new();
    let retained = directory.0.join("injected-unlink-failure");
    fs::create_dir(&retained).unwrap();
    let mut temporary = TemporaryFile {
        path: Some(retained.clone()),
    };
    let error = operation_failure(
        &mut temporary,
        PublishStage::WriteTemporary,
        io::Error::new(io::ErrorKind::WriteZero, "injected write failure"),
    );
    assert!(matches!(error, PublishError::Io {
        stage: PublishStage::WriteTemporary,
        status: PublicationStatus::NotPublished,
        source,
        cleanup: Some(CleanupFailure { path, .. }),
    } if source.kind() == io::ErrorKind::WriteZero && path == retained));
}


#[test]
fn postpublication_cleanup_and_directory_sync_failures_are_independent() {
    for cleanup_fails in [false, true] {
        for sync_fails in [false, true] {
            let cleanup = cleanup_fails.then(|| CleanupFailure {
                path: PathBuf::from("private-name"),
                source: io::Error::other("injected unlink failure"),
            });
            let sync = if sync_fails {
                Err(io::Error::other("injected directory sync failure"))
            } else {
                Ok(())
            };
            let result = publication_result(cleanup, sync);
            if cleanup_fails || sync_fails {
                let error = result.unwrap_err();
                assert_eq!(error.publication_status(), PublicationStatus::Published);
                assert!(error.source().is_some());
                assert!(
                    matches!(error, PublishError::Published { cleanup, directory_sync }
                    if cleanup.is_some() == cleanup_fails && directory_sync.is_some() == sync_fails)
                );
            } else {
                assert!(result.is_ok());
            }
        }
    }
}


#[test]
fn bounded_display_does_not_echo_paths_or_original_causes() {
    let error = PublishError::Published {
        cleanup: Some(CleanupFailure {
            path: PathBuf::from("private-path-must-not-appear"),
            source: io::Error::other("secret cause"),
        }),
        directory_sync: Some(io::Error::other("sensitive directory cause")),
    };
    let message = error.to_string();
    assert!(message.len() < 160);
    assert!(!message.contains("private-path"));
    assert!(!message.contains("secret"));
    assert!(!message.contains("sensitive"));
    assert!(message.contains("published"));
    assert!(message.contains("durability unconfirmed"));
}
