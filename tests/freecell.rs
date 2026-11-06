#![cfg(feature = "freecell_json")]
use xsar::freecell_json as json;
use xsar::games::{
    cards::{Card, Rank, Suit},
    freecell::*,
};
fn deal() -> InitialDeal {
    let cards: Vec<_> = Suit::ALL
        .into_iter()
        .flat_map(|s| Rank::ALL.into_iter().map(move |r| Card::new(r, s)))
        .collect();
    let mut offset = 0;
    let stacks = INITIAL_LENGTHS.map(|n| {
        let stack = CascadeStack::from_cards(cards[offset..offset + n].iter().copied()).unwrap();
        offset += n;
        stack
    });
    InitialDeal::new(
        BoardState::new(stacks, [None; 4], [0; 4]).unwrap(),
        Metadata::new("test", "native", "fixture").unwrap(),
    )
    .unwrap()
}

#[test]
fn codec_strict_objects_and_round_trip() {
    let original = deal();
    let bytes = json::to_vec(&original).unwrap();
    assert_eq!(json::from_slice(&bytes).unwrap(), original);
    let text = String::from_utf8(bytes).unwrap();
    for damaged in [
        text.replacen("\"schema\": 1", "\"schema\": 1, \"schema\": 1", 1),
        text.replacen("\"game\": \"freecell\"", "\"game\": \"pyramid\"", 1),
        text.replacen("\"clubs\": 0", "\"clubs\": 0, \"extra\": 0", 1),
        text.replacen("\"clubs\": 0", "\"clubs\": 0, \"clubs\": 0", 1),
        text.replacen("\"rank\": \"A\"", "\"rank\": \"A\", \"rank\": \"A\"", 1),
        format!("{text} null"),
    ] {
        assert!(json::from_slice(damaged.as_bytes()).is_err(), "{damaged}");
    }
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["provenance"] = serde_json::json!(["test", "native", "fixture"]);
    assert!(json::from_slice(&serde_json::to_vec(&value).unwrap()).is_err());
    value["provenance"] =
        serde_json::json!({"deal_id":"test","profile":"native","source":"fixture"});
    value["foundations"] = serde_json::json!([0, 0, 0, 0]);
    assert!(json::from_slice(&serde_json::to_vec(&value).unwrap()).is_err());
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let array: Vec<_> = [
        "schema",
        "game",
        "cascades",
        "cells",
        "foundations",
        "provenance",
    ]
    .map(|k| value[k].clone())
    .into();
    assert!(json::from_slice(&serde_json::to_vec(&array).unwrap()).is_err());
}
#[test]
fn codec_byte_limit_and_metadata_are_bounded() {
    let mut bytes = json::to_vec(&deal()).unwrap();
    bytes.resize(json::MAX_DOCUMENT_BYTES, b' ');
    assert!(json::from_slice(&bytes).is_ok());
    bytes.push(b' ');
    assert!(matches!(
        json::from_slice(&bytes),
        Err(json::JsonError::InputTooLarge)
    ));
    let mut cursor = std::io::Cursor::new(vec![b' '; 100_000]);
    assert!(json::read_from(&mut cursor).is_err());
    assert_eq!(cursor.position(), 65_537);
    assert!(Metadata::new("/host/path", "p", "s").is_err());
    assert!(Metadata::new("x".repeat(129), "p", "s").is_err());
}
#[cfg(unix)]
#[test]
fn publication_refuses_existing_and_preserves_unusual_path() {
    let path = std::env::temp_dir().join(format!(
        "freecell-json-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let target = path.join("-déjà lu.json");
    json::write_new(&target, &deal()).unwrap();
    let first = std::fs::read(&target).unwrap();
    assert!(json::write_new(&target, &deal()).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), first);
    assert_eq!(json::from_slice(&first).unwrap(), deal());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}

/// Every JSON object is map-only and closed, including nested identities and provenance.
#[test]
fn every_object_rejects_duplicate_unknown_missing_and_array_forms() {
    let bytes = json::to_vec(&deal()).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for (needle, replacement) in [
        (
            "\"game\": \"freecell\"",
            "\"game\": \"freecell\", \"unrecognised\": 1",
        ),
        ("\"rank\": \"A\"", "\"rank\": \"A\", \"extra\": 1"),
        (
            "\"suit\": \"clubs\"",
            "\"suit\": \"clubs\", \"suit\": \"clubs\"",
        ),
        (
            "\"deal_id\": \"test\"",
            "\"deal_id\": \"test\", \"deal_id\": \"other\"",
        ),
        (
            "\"profile\": \"native\"",
            "\"profile\": \"native\", \"extra\": 1",
        ),
    ] {
        let damaged = text.replacen(needle, replacement, 1);
        assert_ne!(damaged, text, "fixture selector {needle}");
        assert!(json::from_slice(damaged.as_bytes()).is_err());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (kind, field) in [
        ("root", "schema"),
        ("card", "suit"),
        ("foundations", "hearts"),
        ("provenance", "profile"),
    ] {
        let mut damaged = value.clone();
        let object = match kind {
            "root" => damaged.as_object_mut().unwrap(),
            "card" => damaged["cascades"][0][0].as_object_mut().unwrap(),
            other => damaged[other].as_object_mut().unwrap(),
        };
        object.remove(field);
        assert!(
            json::from_slice(&serde_json::to_vec(&damaged).unwrap()).is_err(),
            "missing {kind}.{field}"
        );
    }
    for (path, object) in [
        ("card", serde_json::json!(["A", "clubs"])),
        (
            "provenance",
            serde_json::json!(["test", "native", "fixture"]),
        ),
        ("foundations", serde_json::json!([0, 0, 0, 0])),
    ] {
        let mut damaged = value.clone();
        if path == "card" {
            damaged["cascades"][0][0] = object;
        } else {
            damaged[path] = object;
        }
        assert!(json::from_slice(&serde_json::to_vec(&damaged).unwrap()).is_err());
    }
    for cells in [
        serde_json::json!([]),
        serde_json::json!([null, null, null]),
        serde_json::json!([null, null, null, null, null]),
        serde_json::json!([{}, null, null, null]),
    ] {
        let mut damaged = value.clone();
        damaged["cells"] = cells;
        assert!(json::from_slice(&serde_json::to_vec(&damaged).unwrap()).is_err());
    }
    for (field, bad) in [("rank", "joker"), ("suit", "unknown")] {
        let mut damaged = value.clone();
        damaged["cascades"][0][0][field] = bad.into();
        assert!(json::from_slice(&serde_json::to_vec(&damaged).unwrap()).is_err());
    }
    let mut damaged = value.clone();
    damaged["cascades"][0] = serde_json::Value::Array(vec![value["cascades"][0][0].clone(); 21]);
    assert!(json::from_slice(&serde_json::to_vec(&damaged).unwrap()).is_err());
}



/// A pre-existing directory or dangling symlink is never overwritten, even on uncertain link failure.
#[test]
#[cfg(unix)]
fn publication_existing_entries_keep_uncertain_status_without_overwrite() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!(
        "freecell-collision-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let directory = root.join("existing-dir");
    std::fs::create_dir(&directory).unwrap();
    let link = root.join("dangling");
    symlink("never-created", &link).unwrap();
    for target in [&directory, &link] {
        let failure = json::write_new(target, &deal()).unwrap_err();
        assert!(matches!(
            failure,
            json::PublishError::Io {
                status: json::PublicationStatus::Uncertain,
                ..
            }
        ));
    }
    assert!(directory.is_dir());
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        std::path::PathBuf::from("never-created")
    );
    assert!(!root.join("never-created").exists());
    std::fs::remove_dir_all(root).unwrap();
}

/// Non-Unix publication refuses before creating any parent or destination.
#[cfg(not(unix))]
#[test]
fn publication_is_explicitly_unsupported_without_filesystem_side_effects() {
    let root =
        std::env::temp_dir().join(format!("xsar-freecell-unsupported-{}", std::process::id()));
    let target = root.join("new.json");
    assert!(!root.exists());
    assert!(matches!(
        json::write_new(&target, &deal()),
        Err(json::PublishError::UnsupportedPlatform)
    ));
    assert!(!root.exists());
}
