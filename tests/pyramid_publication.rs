//! Real filesystem regressions for the additive public no-clobber publisher.
//!
//! All writable test directories belong to this crate's temporary checkout.
//! The shared complete fixture is synthetic, not live reconnaissance evidence.

#![cfg(all(feature = "pyramid_json", unix))]

use std::{
    ffi::OsString,
    fs::{self, File},
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, DirBuilderExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread,
};
use xsar::{
    games::pyramid::{CompleteDeal, DealMetadata, ProvenanceKind, Restoration},
    pyramid_json::{
        from_slice, read_from, to_vec, write_new, PublicationStatus, PublishError, PublishStage,
    },
};


/// Existing complete synthetic schema fixture; never mutated by publication tests.
const FIXTURE: &[u8] = include_bytes!("fixtures/pyramid-synthetic-v1.json");


/// Publication upgrades legacy input to independently authored canonical schema 2.
const OUTPUT_FIXTURE: &[u8] = include_bytes!("fixtures/pyramid-synthetic-v2.json");


/// Independent test-directory names within a single test process.
static DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(0);


/// One exclusive private test directory, removed after its test completes.
struct TestDirectory(PathBuf);


impl TestDirectory {
    /// Allocate a fresh namespace without sharing paths with other tests.
    fn new() -> Self {
        let sequence = DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            ".publisher-integration-{}-{sequence}",
            std::process::id()
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .expect("create test directory");
        Self(directory)
    }


    /// Join a literal test name without changing the process working directory.
    fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }


    /// Assert no unreported temporary links or unexpected files remain.
    fn entry_count(&self) -> usize {
        fs::read_dir(&self.0).expect("read test directory").count()
    }
}


impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}


/// Load the same validated deal used by the existing codec acceptance tests.
fn fixture() -> CompleteDeal {
    from_slice(FIXTURE).expect("valid complete synthetic fixture")
}


/// Preserve identities while assigning each racing producer distinguishable bytes.
fn identified_fixture(identifier: &str) -> CompleteDeal {
    let original = fixture();
    let metadata = DealMetadata::try_new(
        identifier,
        ProvenanceKind::SyntheticFixture,
        Restoration::NotApplicable,
        "publisher-tests",
        "synthetic concurrency fixture",
        None,
    )
    .expect("valid synthetic metadata");
    CompleteDeal::try_new(*original.tableau(), *original.stock(), metadata).expect("valid deal")
}


#[test]
fn published_file_round_trips_with_private_mode_and_no_residue() {
    let directory = TestDirectory::new();
    let destination = directory.join("complete.json");
    let deal = fixture();
    write_new(&destination, &deal).expect("publish new file");
    assert_eq!(fs::read(&destination).unwrap(), to_vec(&deal).unwrap());
    assert_eq!(fs::read(&destination).unwrap(), OUTPUT_FIXTURE);
    assert_eq!(read_from(File::open(&destination).unwrap()).unwrap(), deal);
    let mode = fs::metadata(&destination).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode & !0o600, 0, "no group, other or executable bits");
    assert_eq!(mode, 0o600, "normal test umask preserves owner read/write");
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn existing_regular_destination_is_never_changed() {
    let directory = TestDirectory::new();
    let destination = directory.join("existing.json");
    fs::write(&destination, b"keep the existing bytes").unwrap();
    let error = write_new(&destination, &fixture()).unwrap_err();
    assert!(matches!(error, PublishError::Io {
        stage: PublishStage::LinkDestination,
        status: PublicationStatus::Uncertain,
        ref source,
        cleanup: None,
    } if source.kind() == std::io::ErrorKind::AlreadyExists));
    assert_eq!(fs::read(destination).unwrap(), b"keep the existing bytes");
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn existing_destination_symlink_is_not_followed_or_replaced() {
    let directory = TestDirectory::new();
    let original = directory.join("original.json");
    let destination = directory.join("linked.json");
    fs::write(&original, b"untouched target").unwrap();
    symlink(&original, &destination).unwrap();
    assert!(write_new(&destination, &fixture()).is_err());
    assert_eq!(fs::read_link(&destination).unwrap(), original);
    assert_eq!(fs::read(&original).unwrap(), b"untouched target");
    assert_eq!(directory.entry_count(), 2);
}


#[test]
fn dangling_destination_symlink_is_not_followed_or_replaced() {
    let directory = TestDirectory::new();
    let absent = directory.join("absent.json");
    let destination = directory.join("dangling.json");
    symlink(&absent, &destination).unwrap();
    assert!(write_new(&destination, &fixture()).is_err());
    assert_eq!(fs::read_link(&destination).unwrap(), absent);
    assert!(!absent.exists());
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn existing_destination_directory_is_not_removed() {
    let directory = TestDirectory::new();
    let destination = directory.join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"retain nested entry").unwrap();
    assert!(write_new(&destination, &fixture()).is_err());
    assert_eq!(
        fs::read(destination.join("keep")).unwrap(),
        b"retain nested entry"
    );
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn invalid_filename_shapes_refuse_before_creating_a_temporary() {
    let directory = TestDirectory::new();
    let deal = fixture();
    for path in [
        PathBuf::from(""),
        PathBuf::from("/"),
        directory.join("."),
        directory.join(".."),
        directory.join("trailing/"),
    ] {
        let error = write_new(path, &deal).unwrap_err();
        assert!(matches!(error, PublishError::InvalidPath));
        assert_eq!(error.publication_status(), PublicationStatus::NotPublished);
        assert_eq!(directory.entry_count(), 0);
    }
    let nul = directory.join(OsString::from_vec(b"bad\0name.json".to_vec()));
    assert!(matches!(
        write_new(nul, &deal),
        Err(PublishError::InvalidPath)
    ));
    assert_eq!(directory.entry_count(), 0);
}


#[test]
fn missing_parent_is_not_created() {
    let directory = TestDirectory::new();
    let error = write_new(directory.join("missing/deal.json"), &fixture()).unwrap_err();
    assert!(matches!(
        error,
        PublishError::Io {
            stage: PublishStage::OpenDirectory,
            status: PublicationStatus::NotPublished,
            cleanup: None,
            ..
        }
    ));
    assert_eq!(directory.entry_count(), 0);
}


#[test]
fn regular_file_parent_fails_without_touching_its_bytes() {
    let directory = TestDirectory::new();
    let parent = directory.join("not-a-directory");
    fs::write(&parent, b"unchanged").unwrap();
    let error = write_new(parent.join("deal.json"), &fixture()).unwrap_err();
    assert!(matches!(
        error,
        PublishError::Io {
            stage: PublishStage::OpenDirectory,
            ..
        }
    ));
    assert_eq!(fs::read(parent).unwrap(), b"unchanged");
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn non_utf8_destination_names_preserve_native_path_bytes() {
    let directory = TestDirectory::new();
    let destination = directory.join(OsString::from_vec(b"deal-\xff.json".to_vec()));
    write_new(&destination, &fixture()).unwrap();
    assert_eq!(fs::read(destination).unwrap(), OUTPUT_FIXTURE);
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn overlong_destination_name_cleans_its_private_temporary() {
    let directory = TestDirectory::new();
    let error = write_new(directory.join("x".repeat(4096)), &fixture()).unwrap_err();
    assert!(matches!(
        error,
        PublishError::Io {
            stage: PublishStage::LinkDestination,
            status: PublicationStatus::Uncertain,
            cleanup: None,
            ..
        }
    ));
    assert_eq!(directory.entry_count(), 0);
}


#[test]
fn concurrent_publishers_have_exactly_one_complete_winner() {
    let directory = TestDirectory::new();
    let destination = directory.join("winner.json");
    let participants = 16;
    let barrier = Arc::new(Barrier::new(participants));
    let results = thread::scope(|scope| {
        let handles: Vec<_> = (0..participants)
            .map(|ordinal| {
                let barrier = Arc::clone(&barrier);
                let destination = &destination;
                scope.spawn(move || {
                    let deal = identified_fixture(&format!("writer-{ordinal}"));
                    barrier.wait();
                    let result = write_new(destination, &deal);
                    (deal, result)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    let winners: Vec<_> = results
        .iter()
        .filter(|(_, result)| result.is_ok())
        .collect();
    assert_eq!(winners.len(), 1);
    let actual = read_from(File::open(&destination).unwrap()).unwrap();
    assert_eq!(actual, winners[0].0);
    for (_, result) in results.iter().filter(|(_, result)| result.is_err()) {
        assert!(matches!(result, Err(PublishError::Io {
            stage: PublishStage::LinkDestination,
            source,
            cleanup: None,
            ..
        }) if source.kind() == std::io::ErrorKind::AlreadyExists));
    }
    assert_eq!(directory.entry_count(), 1);
}


#[test]
fn repeat_publication_preserves_the_original_valid_deal() {
    let directory = TestDirectory::new();
    let destination = directory.join("repeat.json");
    let first = identified_fixture("first");
    write_new(&destination, &first).unwrap();
    assert!(write_new(&destination, &identified_fixture("second")).is_err());
    assert_eq!(read_from(File::open(destination).unwrap()).unwrap(), first);
    assert_eq!(directory.entry_count(), 1);
}
