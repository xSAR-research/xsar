//! Strict legacy schema-1 reading and schema-2 writing of a synthetic deal.
//!
//! This fixture proves codec and logical-order behaviour, never screen recognition
//! or live restoration. All malformed inputs and readers are finite and local.
//! Tests deliberately avoid private wire types and JsonError variant assumptions.

#![cfg(feature = "pyramid_json")]

use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, Cursor, Read};
use xsar::games::pyramid::{CompleteDeal, ProvenanceKind, Restoration};
use xsar::pyramid_json::{from_slice, read_from, to_vec, JsonError, MAX_DOCUMENT_BYTES};


/// Independently authored canonical bytes, including a final newline.
const FIXTURE: &[u8] = include_bytes!("fixtures/pyramid-synthetic-v1.json");


/// Independently authored schema-2 bytes preserve the original deal and provenance.
const OUTPUT_FIXTURE: &[u8] = include_bytes!("fixtures/pyramid-synthetic-v2.json");


/// Explicit logical rows; these expectations do not enumerate the model's deck.
const EXPECTED_ROWS: [&[(&str, &str)]; 7] = [
    &[("A", "clubs")],
    &[("2", "clubs"), ("3", "clubs")],
    &[("4", "clubs"), ("5", "clubs"), ("6", "clubs")],
    &[
        ("7", "clubs"),
        ("8", "clubs"),
        ("9", "clubs"),
        ("10", "clubs"),
    ],
    &[
        ("J", "clubs"),
        ("Q", "clubs"),
        ("K", "clubs"),
        ("A", "diamonds"),
        ("2", "diamonds"),
    ],
    &[
        ("3", "diamonds"),
        ("4", "diamonds"),
        ("5", "diamonds"),
        ("6", "diamonds"),
        ("7", "diamonds"),
        ("8", "diamonds"),
    ],
    &[
        ("9", "diamonds"),
        ("10", "diamonds"),
        ("J", "diamonds"),
        ("Q", "diamonds"),
        ("K", "diamonds"),
        ("A", "hearts"),
        ("2", "hearts"),
    ],
];


/// Every first-draw ordinal, specified independently of the fixture's parser.
const EXPECTED_STOCK: [(&str, &str); 24] = [
    ("3", "hearts"),
    ("4", "hearts"),
    ("5", "hearts"),
    ("6", "hearts"),
    ("7", "hearts"),
    ("8", "hearts"),
    ("9", "hearts"),
    ("10", "hearts"),
    ("J", "hearts"),
    ("Q", "hearts"),
    ("K", "hearts"),
    ("A", "spades"),
    ("2", "spades"),
    ("3", "spades"),
    ("4", "spades"),
    ("5", "spades"),
    ("6", "spades"),
    ("7", "spades"),
    ("8", "spades"),
    ("9", "spades"),
    ("10", "spades"),
    ("J", "spades"),
    ("Q", "spades"),
    ("K", "spades"),
];


/// Exercise each object kind and both ends of each card collection.
const OBJECT_PATHS: [&str; 6] = [
    "",
    "/provenance",
    "/tableau/0",
    "/tableau/27",
    "/stock/0",
    "/stock/23",
];


/// Metadata paths and byte limits fixed by the public schema, not model constants.
const METADATA_LIMITS: [(&str, usize); 4] = [
    ("/deal_id", 64),
    ("/provenance/producer", 64),
    ("/provenance/source", 256),
    ("/provenance/evidence_id", 128),
];


/// Decode only test mutation scaffolding into an unrestricted JSON value.
fn fixture_value() -> Value {
    serde_json::from_slice(FIXTURE).expect("the authored fixture is valid JSON")
}


/// Encode test-owned mutations without prescribing public writer behaviour.
fn encode(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("test values are serialisable")
}


/// Read through both public input APIs and require the same complete value.
fn accept(bytes: &[u8]) -> CompleteDeal {
    let deal = from_slice(bytes).expect("slice reader accepts the valid contract");
    assert_eq!(
        read_from(Cursor::new(bytes)).expect("stream reader accepts"),
        deal
    );
    deal
}


/// Require useful, bounded Display output without coupling to variant names.
fn assert_error_sane(error: &JsonError) {
    let message = error.to_string();
    assert!(
        !message.trim().is_empty(),
        "an error needs a useful description"
    );
    assert!(
        message.len() <= 1024,
        "error Display must stay bounded: {} bytes",
        message.len()
    );
}


/// Require refusal through both public readers without standardising precedence.
fn reject(bytes: &[u8]) {
    for result in [from_slice(bytes), read_from(Cursor::new(bytes))] {
        let error = result.expect_err("invalid contract input must be refused");
        assert_error_sane(&error);
    }
}


/// Replace one fixture node with raw JSON, preserving duplicate keys or bad syntax.
fn raw_replacement(pointer: &str, replacement: &str) -> Vec<u8> {
    let mut document = fixture_value();
    *document
        .pointer_mut(pointer)
        .expect("known fixture pointer") = json!("RAW_CONTRACT_NODE");
    let text = serde_json::to_string(&document).expect("test document encodes");
    assert_eq!(text.matches("\"RAW_CONTRACT_NODE\"").count(), 1);
    text.replacen("\"RAW_CONTRACT_NODE\"", replacement, 1)
        .into_bytes()
}


/// Pad one complete fixture to an exact encoded byte count with legal whitespace.
fn padded_document(length: usize) -> Vec<u8> {
    assert!(length >= FIXTURE.len());
    let mut bytes = FIXTURE.to_vec();
    bytes.resize(length, b' ');
    bytes
}


/// Count short reads, inject finite interruptions and fail at a selected offset.
struct TestReader<'bytes> {
    /// Finite backing bytes; no filesystem or live stream is used.
    bytes: &'bytes [u8],
    /// Number of bytes actually handed to the codec.
    position: usize,
    /// Positive upper bound for each successful read.
    chunk_size: usize,
    /// Calls, including EOF, interruption and failure observations.
    calls: usize,
    /// Remaining interruptions, interleaved with successful reads.
    interruptions_left: usize,
    /// Optional delivered-byte offset at which a non-interrupted error occurs.
    fail_at: Option<usize>,
}


impl<'bytes> TestReader<'bytes> {
    /// Construct a finite reader with observable progress and no initial faults.
    fn new(bytes: &'bytes [u8], chunk_size: usize) -> Self {
        assert!(chunk_size > 0);
        Self {
            bytes,
            position: 0,
            chunk_size,
            calls: 0,
            interruptions_left: 0,
            fail_at: None,
        }
    }
}


impl Read for TestReader<'_> {
    /// Fail promptly on a retry loop instead of letting a regression hang tests.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        assert!(
            self.calls <= MAX_DOCUMENT_BYTES + 32,
            "reader retry budget exceeded"
        );


        if buffer.is_empty() {
            return Ok(0);
        }


        if self.interruptions_left > 0 && self.calls % 2 == 1 {
            self.interruptions_left -= 1;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }


        if self.fail_at.is_some_and(|offset| self.position >= offset) {
            return Err(io::Error::other("injected contract read failure"));
        }
        let mut count = buffer
            .len()
            .min(self.chunk_size)
            .min(self.bytes.len() - self.position);


        if let Some(offset) = self.fail_at {
            count = count.min(offset - self.position);
        }
        buffer[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}


/// Simulate a non-EOF stream with a finite tripwire after the permitted byte probe.
#[derive(Default)]
struct WhitespaceStream {
    /// Bytes delivered before any attempted read beyond the contract allowance.
    delivered: usize,
    /// Calls are independently bounded so repeated empty reads cannot hang.
    calls: usize,
}


impl Read for WhitespaceStream {
    /// Supply whitespace without EOF, panicking on a read after 65,537 bytes.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        assert!(
            self.calls <= MAX_DOCUMENT_BYTES + 2,
            "non-EOF read loop exceeded its budget"
        );


        if buffer.is_empty() {
            return Ok(0);
        }
        assert!(
            self.delivered < MAX_DOCUMENT_BYTES + 1,
            "read beyond the oversize probe"
        );
        let count = buffer.len().min(MAX_DOCUMENT_BYTES + 1 - self.delivered);
        buffer[..count].fill(b' ');
        self.delivered += count;
        Ok(count)
    }
}


/// Both readers preserve every independently specified logical row and ordinal.
#[test]
fn fixture_has_all_expected_rows_stock_ordinals_and_declared_origin() {
    let deal = accept(FIXTURE);
    let mut index = 0;


    for (row, expected_cards) in EXPECTED_ROWS.iter().enumerate() {
        assert_eq!(expected_cards.len(), row + 1);


        for (column, expected) in expected_cards.iter().enumerate() {
            let card = deal.card_at(row, column).expect("valid logical position");
            assert_eq!(
                (card.rank().as_str(), card.suit().as_str()),
                *expected,
                "row {row}, column {column}"
            );
            assert_eq!(deal.tableau()[index], card, "tableau index {index}");
            index += 1;
        }
    }
    assert_eq!(index, 28);
    assert_eq!(deal.stock().len(), EXPECTED_STOCK.len());


    for (ordinal, expected) in EXPECTED_STOCK.iter().enumerate() {
        let card = deal.stock()[ordinal];
        assert_eq!(
            (card.rank().as_str(), card.suit().as_str()),
            *expected,
            "stock ordinal {ordinal}"
        );
    }
    let identities: HashSet<_> = deal.tableau().iter().chain(deal.stock()).copied().collect();
    assert_eq!(identities.len(), 52);
    let metadata = deal.metadata();
    assert_eq!(metadata.deal_id(), "synthetic-ordered-deck-v1");
    assert_eq!(metadata.provenance(), ProvenanceKind::SyntheticFixture);
    assert_eq!(metadata.restoration(), Restoration::NotApplicable);
    assert_eq!(metadata.producer(), "xsar foundation contract tests");
    assert_eq!(
        metadata.source(),
        "Manually authored synthetic deck; no screen recognition or live restoration."
    );
    assert_eq!(metadata.evidence_id(), Some("synthetic-deck-v1"));
}


/// Coordinate validation remains available to an external JSON consumer.
#[test]
fn public_lookup_refuses_out_of_range_and_overflow_sized_coordinates() {
    let deal = accept(FIXTURE);


    for (row, column) in [(0, 1), (6, 7), (7, 0), (usize::MAX, 0), (6, usize::MAX)] {
        assert!(deal.card_at(row, column).is_err());
    }
}


/// Legacy input migrates to the authored schema-2 indentation, order and newline.
#[test]
fn writer_matches_independent_canonical_fixture_and_semantic_round_trip() {
    let deal = accept(FIXTURE);
    let output = to_vec(&deal).expect("bounded complete deal serialises");
    assert_eq!(output, OUTPUT_FIXTURE);
    assert_eq!(to_vec(&deal).expect("repeat encoding succeeds"), output);
    assert_eq!(accept(&output), deal);
    assert_eq!(
        to_vec(&accept(&output)).expect("second round trip encodes"),
        output
    );
    assert!(std::str::from_utf8(&output).is_ok());
    assert!(output.len() <= MAX_DOCUMENT_BYTES);
}


/// Input object order and insignificant whitespace do not change deal identity.
#[test]
fn compact_reordered_keys_and_escaped_equivalent_strings_are_accepted() {
    let compact = encode(&fixture_value());
    assert_ne!(compact, FIXTURE);
    assert_eq!(accept(&compact), accept(FIXTURE));
    let escaped = raw_replacement("/tableau/0/rank", r#""\u0041""#);
    assert_eq!(accept(&escaped), accept(FIXTURE));
    let escaped_game = raw_replacement("/game", r#""pyra\u006did""#);
    assert_eq!(accept(&escaped_game), accept(FIXTURE));
    let spaced = [b" \r\n\t".as_slice(), FIXTURE, b" \r\n\t".as_slice()].concat();
    assert_eq!(
        to_vec(&accept(&spaced)).expect("canonical encoding"),
        OUTPUT_FIXTURE
    );
}


/// Array order is authoritative and must survive decoding and re-encoding.
#[test]
fn reader_and_writer_preserve_a_valid_non_fixture_permutation() {
    let mut document = fixture_value();
    let original = accept(FIXTURE);
    let tableau_card = document["tableau"][0].clone();
    document["tableau"][0] = document["stock"][23].clone();
    document["stock"][23] = tableau_card;
    let deal = accept(&encode(&document));
    assert_ne!(deal, original);
    assert_eq!(deal.tableau()[0], original.stock()[23]);
    assert_eq!(deal.stock()[23], original.tableau()[0]);
    assert_eq!(accept(&to_vec(&deal).expect("permuted deal encodes")), deal);
}


/// Omission is the sole representation of absent optional evidence.
#[test]
fn evidence_may_be_omitted_but_may_not_be_null() {
    let mut document = fixture_value();
    document["provenance"]
        .as_object_mut()
        .expect("provenance object")
        .remove("evidence_id");
    let deal = accept(&encode(&document));
    assert_eq!(deal.metadata().evidence_id(), None);
    let output = to_vec(&deal).expect("omitted evidence encodes");
    let wire: Value = serde_json::from_slice(&output).expect("writer emits JSON");
    assert!(wire["provenance"].get("evidence_id").is_none());
    assert_eq!(accept(&output), deal);
    document["provenance"]["evidence_id"] = Value::Null;
    reject(&encode(&document));
}


/// Screen restoration declarations and fixture declarations cannot be interchanged.
#[test]
fn all_provenance_restoration_combinations_are_checked() {
    for (name, kind, valid_restoration) in [
        (
            "synthetic_fixture",
            ProvenanceKind::SyntheticFixture,
            "not_applicable",
        ),
        (
            "manual_fixture",
            ProvenanceKind::ManualFixture,
            "not_applicable",
        ),
        (
            "screen_recognition",
            ProvenanceKind::ScreenRecognition,
            "verified",
        ),
    ] {
        for restoration in ["not_applicable", "verified"] {
            let mut document = fixture_value();
            document["provenance"]["kind"] = json!(name);
            document["restoration"] = json!(restoration);


            if restoration == valid_restoration {
                let deal = accept(&encode(&document));
                assert_eq!(deal.metadata().provenance(), kind);
                let expected = if restoration == "verified" {
                    Restoration::Verified
                } else {
                    Restoration::NotApplicable
                };
                assert_eq!(deal.metadata().restoration(), expected);
                assert_eq!(
                    accept(&to_vec(&deal).expect("declared origin encodes")),
                    deal
                );
            } else {
                reject(&encode(&document));
            }
        }
    }
}


/// Required fields cannot disappear or acquire implicit null defaults at any level.
#[test]
fn missing_and_null_required_fields_are_refused_at_every_object_level() {
    let original = fixture_value();


    for pointer in OBJECT_PATHS {
        let object = original
            .pointer(pointer)
            .expect("known object")
            .as_object()
            .expect("object");


        for key in object.keys().filter(|key| key.as_str() != "evidence_id") {
            let mut missing = original.clone();
            missing
                .pointer_mut(pointer)
                .expect("known object")
                .as_object_mut()
                .expect("object")
                .remove(key);
            reject(&encode(&missing));
            let mut null = original.clone();
            null.pointer_mut(pointer).expect("known object")[key] = Value::Null;
            reject(&encode(&null));
        }
    }
}


/// Exact duplicate values still fail; duplicate keys cannot silently overwrite.
#[test]
fn duplicate_keys_are_refused_for_every_field_and_object_level() {
    let original = fixture_value();


    for pointer in OBJECT_PATHS {
        let object = original.pointer(pointer).expect("known object");


        for (key, value) in object.as_object().expect("object") {
            let mut text = serde_json::to_string(object).expect("object encodes");
            assert_eq!(text.pop(), Some('}'));
            text.push(',');
            text.push_str(&serde_json::to_string(key).expect("key encodes"));
            text.push(':');
            text.push_str(&serde_json::to_string(value).expect("value encodes"));
            text.push('}');
            reject(&raw_replacement(pointer, &text));
        }
    }
}


/// Decoded-equivalent field names are duplicates, and conflicting values also fail.
#[test]
fn escaped_and_conflicting_duplicate_keys_are_refused() {
    for object in [
        r#"{"rank":"A","\u0072ank":"A","suit":"clubs"}"#,
        r#"{"rank":"K","rank":"A","suit":"clubs"}"#,
        r#"{"rank":"A","suit":"diamonds","suit":"clubs"}"#,
    ] {
        reject(&raw_replacement("/tableau/0", object));
    }
}


/// Native coordinates, ordinals and arbitrary extensions are outside schema 1.
#[test]
fn unknown_keys_are_refused_at_every_object_level() {
    for pointer in OBJECT_PATHS {
        for key in [
            "unknown",
            "row",
            "ordinal",
            "capture",
            "schema_version_typo",
        ] {
            let mut document = fixture_value();
            document.pointer_mut(pointer).expect("known object")[key] = json!(0);
            reject(&encode(&document));
        }
    }
}


/// Serde's positional-struct representation must not expand the JSON contract.
#[test]
fn root_provenance_and_cards_refuse_positional_arrays() {
    let original = fixture_value();
    let root_fields = [
        "schema_version",
        "game",
        "deal_id",
        "complete",
        "restoration",
        "provenance",
        "tableau",
        "stock",
    ];
    let root_array: Vec<_> = root_fields
        .iter()
        .map(|field| original[*field].clone())
        .collect();
    reject(&encode(&json!(root_array)));
    let provenance_fields = ["kind", "producer", "source", "evidence_id"];
    let provenance_array: Vec<_> = provenance_fields
        .iter()
        .map(|field| original["provenance"][*field].clone())
        .collect();
    reject(&raw_replacement(
        "/provenance",
        &serde_json::to_string(&provenance_array).expect("test array"),
    ));


    for pointer in ["/tableau/0", "/tableau/27", "/stock/0", "/stock/23"] {
        let card = original.pointer(pointer).expect("known card");
        let array = json!([card["rank"], card["suit"]]);
        reject(&raw_replacement(
            pointer,
            &serde_json::to_string(&array).expect("test array"),
        ));
    }
}


/// Object-only fields also reject scalars, empty arrays and nulls.
#[test]
fn object_fields_refuse_every_other_json_shape() {
    for pointer in OBJECT_PATHS {
        for wrong_shape in [
            json!(null),
            json!(false),
            json!(17),
            json!("object"),
            json!([]),
        ] {
            let mut document = fixture_value();
            *document.pointer_mut(pointer).expect("known object") = wrong_shape;
            reject(&encode(&document));
        }
    }
}


/// Neither numeric coercion nor aliases can select the version or game.
#[test]
fn schema_version_and_game_require_exact_supported_values() {
    for wrong in [
        json!(0),
        json!(3),
        json!(-1),
        json!(1.0),
        json!("1"),
        json!(true),
        json!(null),
        json!([]),
        json!({}),
    ] {
        let mut document = fixture_value();
        document["schema_version"] = wrong;
        reject(&encode(&document));
    }


    for number in ["1e0", "1.00", "18446744073709551616", "1e400"] {
        reject(&raw_replacement("/schema_version", number));
    }


    for wrong in [
        json!(""),
        json!("Pyramid"),
        json!("PYRAMID"),
        json!("tripeaks"),
        json!("pyramid "),
        json!(1),
        json!(true),
        json!(null),
        json!([]),
        json!({}),
    ] {
        let mut document = fixture_value();
        document["game"] = wrong;
        reject(&encode(&document));
    }
}


/// Partial inventories and unrecognised restoration/origin labels are never deals.
#[test]
fn completion_restoration_and_provenance_refuse_aliases_and_wrong_types() {
    for wrong in [
        json!(false),
        json!(0),
        json!(1),
        json!("true"),
        json!(null),
        json!([]),
        json!({}),
    ] {
        let mut document = fixture_value();
        document["complete"] = wrong;
        reject(&encode(&document));
    }


    for pointer in ["/restoration", "/provenance/kind"] {
        for wrong in [
            json!(""),
            json!("unknown"),
            json!("Verified"),
            json!("SyntheticFixture"),
            json!(true),
            json!(1),
            json!(null),
            json!([]),
            json!({}),
        ] {
            let mut document = fixture_value();
            *document.pointer_mut(pointer).expect("known declaration") = wrong;
            reject(&encode(&document));
        }
    }
}


/// Every card location refuses noncanonical rank strings and non-string values.
#[test]
fn invalid_rank_spellings_numbers_and_nulls_are_refused() {
    for collection in ["tableau", "stock"] {
        let count = fixture_value()[collection]
            .as_array()
            .expect("card array")
            .len();


        for index in 0..count {
            for wrong in [
                json!("ace"),
                json!("Ace"),
                json!("a"),
                json!("1"),
                json!("02"),
                json!("11"),
                json!("T"),
                json!(" A"),
                json!("A "),
                json!("joker"),
                json!(""),
                json!(1),
                json!(2),
                json!(1.0),
                json!(null),
                json!(false),
                json!([]),
                json!({}),
            ] {
                let mut document = fixture_value();
                document[collection][index]["rank"] = wrong;
                reject(&encode(&document));
            }
        }
    }
}


/// Suits are canonical names, never colour, glyph, number or alias encodings.
#[test]
fn invalid_suit_spellings_numbers_and_nulls_are_refused() {
    for collection in ["tableau", "stock"] {
        let count = fixture_value()[collection]
            .as_array()
            .expect("card array")
            .len();


        for index in 0..count {
            for wrong in [
                json!("Clubs"),
                json!("CLUBS"),
                json!("club"),
                json!("C"),
                json!("red"),
                json!("spade"),
                json!("♠"),
                json!("hearts "),
                json!(""),
                json!(0),
                json!(1.0),
                json!(null),
                json!(true),
                json!([]),
                json!({}),
            ] {
                let mut document = fixture_value();
                document[collection][index]["suit"] = wrong;
                reject(&encode(&document));
            }
        }
    }
}


/// Short, empty and excessive collections cannot be truncated, padded or inferred.
#[test]
fn tableau_and_stock_require_exact_lengths_and_array_shapes() {
    for (collection, required) in [("tableau", 28), ("stock", 24)] {
        for length in [0, 1, required - 1, required + 1, required + 32] {
            let mut document = fixture_value();
            let cards = document[collection].as_array_mut().expect("card array");
            let extra = cards[0].clone();
            cards.resize(length, extra);
            reject(&encode(&document));
        }


        for wrong in [
            json!(null),
            json!(false),
            json!(28),
            json!("cards"),
            json!({}),
            json!({"0": {"rank": "A", "suit": "clubs"}}),
        ] {
            let mut document = fixture_value();
            document[collection] = wrong;
            reject(&encode(&document));
        }
    }
}


/// Duplicates within either collection and across collections leave a missing card.
#[test]
fn duplicate_card_identities_are_refused_within_and_across_collections() {
    for (source, destination) in [
        ("/tableau/0", "/tableau/1"),
        ("/tableau/0", "/tableau/27"),
        ("/stock/0", "/stock/1"),
        ("/stock/0", "/stock/23"),
        ("/tableau/0", "/stock/0"),
        ("/tableau/27", "/stock/23"),
        ("/stock/23", "/tableau/0"),
    ] {
        let mut document = fixture_value();
        *document.pointer_mut(destination).expect("destination card") =
            document.pointer(source).expect("source card").clone();
        reject(&encode(&document));
    }
}


/// Every bounded string accepts its endpoints and refuses empty or oversized data.
#[test]
fn metadata_ascii_byte_boundaries_and_required_string_types_are_enforced() {
    for (pointer, maximum) in METADATA_LIMITS {
        for length in [1, maximum - 1, maximum] {
            let mut document = fixture_value();
            *document.pointer_mut(pointer).expect("metadata field") = json!("x".repeat(length));
            let deal = accept(&encode(&document));
            assert_eq!(
                accept(&to_vec(&deal).expect("boundary metadata encodes")),
                deal
            );
        }


        for wrong in [
            json!(""),
            json!("x".repeat(maximum + 1)),
            json!(null),
            json!(12),
            json!(true),
            json!([]),
            json!({}),
        ] {
            let mut document = fixture_value();
            *document.pointer_mut(pointer).expect("metadata field") = wrong;
            reject(&encode(&document));
        }
    }
}


/// UTF-8 limits count bytes; valid non-ASCII descriptions retain their exact text.
#[test]
fn producer_and_source_use_utf8_byte_limits_not_character_counts() {
    for (pointer, maximum) in [("/provenance/producer", 64), ("/provenance/source", 256)] {
        for glyph in ["é", "🂡"] {
            let value = glyph.repeat(maximum / glyph.len());
            assert_eq!(value.len(), maximum);
            let mut document = fixture_value();
            *document.pointer_mut(pointer).expect("text field") = json!(value);
            let deal = accept(&encode(&document));
            assert_eq!(
                accept(&to_vec(&deal).expect("UTF-8 metadata encodes")),
                deal
            );
            *document.pointer_mut(pointer).expect("text field") = json!(format!("{value}x"));
            reject(&encode(&document));
        }
    }
}


/// Tokens are nonempty ASCII labels, never paths, whitespace or Unicode aliases.
#[test]
fn identifiers_accept_only_the_agreed_ascii_token_alphabet() {
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    assert_eq!(alphabet.len(), 64);


    for pointer in ["/deal_id", "/provenance/evidence_id"] {
        let mut document = fixture_value();
        *document.pointer_mut(pointer).expect("token field") = json!(alphabet);
        accept(&encode(&document));


        for invalid in [
            "a b",
            "a/b",
            "../evidence",
            "a.b",
            "a:b",
            "a\\b",
            "a\tb",
            "a\nb",
            "a\0b",
            "é",
            "Ａ",
            "🂡",
        ] {
            *document.pointer_mut(pointer).expect("token field") = json!(invalid);
            reject(&encode(&document));
        }
    }
}


/// Both text fields reject ASCII and Unicode C1 controls even when JSON-escaped.
#[test]
fn metadata_descriptions_reject_all_control_characters() {
    for pointer in ["/provenance/producer", "/provenance/source"] {
        for scalar in (0_u32..=31).chain(127..=159) {
            let control = char::from_u32(scalar).expect("control scalar");
            let mut document = fixture_value();
            *document.pointer_mut(pointer).expect("text field") =
                json!(format!("before{control}after"));
            reject(&encode(&document));
        }
    }
}


/// Maximum valid strings, including JSON escape expansion, stay bounded and stable.
#[test]
fn writer_handles_maximum_metadata_and_json_escaping_within_output_cap() {
    let mut document = fixture_value();
    document["deal_id"] = json!("d".repeat(64));
    document["provenance"]["producer"] = json!("\\\"".repeat(32));
    document["provenance"]["source"] = json!("\\\"".repeat(128));
    document["provenance"]["evidence_id"] = json!("e".repeat(128));
    let deal = accept(&encode(&document));
    let output = to_vec(&deal).expect("maximum metadata encodes");
    assert!(output.len() <= MAX_DOCUMENT_BYTES);
    assert_eq!(output.last(), Some(&b'\n'));
    assert_eq!(accept(&output), deal);
    assert_eq!(to_vec(&deal).expect("repeat encoding"), output);
}


/// The byte cap includes legal leading/trailing whitespace and the final newline.
#[test]
fn exact_document_cap_is_accepted_and_one_extra_byte_is_refused() {
    assert_eq!(MAX_DOCUMENT_BYTES, 65_536);
    let expected = accept(FIXTURE);


    for length in [MAX_DOCUMENT_BYTES - 1, MAX_DOCUMENT_BYTES] {
        let bytes = padded_document(length);
        assert_eq!(accept(&bytes), expected);
        let leading = [vec![b' '; length - FIXTURE.len()], FIXTURE.to_vec()].concat();
        assert_eq!(accept(&leading), expected);
    }
    reject(&padded_document(MAX_DOCUMENT_BYTES + 1));
    reject(&vec![b' '; MAX_DOCUMENT_BYTES + 1]);
}


/// No second document, punctuation, comment, BOM or arbitrary suffix is accepted.
#[test]
fn trailing_data_and_non_json_extensions_are_refused() {
    for suffix in [
        b"{}".as_slice(),
        b"[]",
        b"null",
        b"0",
        b"x",
        b"\0",
        b",",
        b"// comment",
        FIXTURE,
    ] {
        reject(&[FIXTURE, suffix].concat());
    }
    reject(&[b"\xef\xbb\xbf".as_slice(), FIXTURE].concat());
    reject(b"");
    reject(b" \n\t\r");
    reject(&raw_replacement("/complete", "True"));
    reject(&raw_replacement("/schema_version", "01"));
    reject(&raw_replacement(
        "/tableau/0",
        r#"{"rank":"A","suit":"clubs",}"#,
    ));
}


/// All incomplete prefixes refuse; omitting only final whitespace remains valid.
#[test]
fn truncated_document_prefixes_are_refused_without_panicking() {
    let complete_length = std::str::from_utf8(FIXTURE)
        .expect("UTF-8 fixture")
        .trim_end()
        .len();


    for length in 0..complete_length {
        reject(&FIXTURE[..length]);
    }
    assert_eq!(accept(&FIXTURE[..complete_length]), accept(FIXTURE));
}


/// Invalid UTF-8 and unpaired surrogate escapes fail before producing a model.
#[test]
fn malformed_utf8_and_unicode_escapes_are_refused() {
    for invalid in [
        vec![0x80],
        vec![0xff],
        vec![0xc0, 0xaf],
        vec![0xed, 0xa0, 0x80],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, 0x82],
    ] {
        let mut bytes = raw_replacement("/provenance/source", "\"UTF8_SENTINEL\"");
        let marker = b"UTF8_SENTINEL";
        let offset = bytes
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("marker exists");
        bytes.splice(offset..offset + marker.len(), invalid);
        reject(&bytes);
    }


    for invalid in [r#""\uD800""#, r#""\uDC00""#, r#""\uD800x""#, r#""\uZZZZ""#] {
        reject(&raw_replacement("/provenance/source", invalid));
    }
}


/// Deeply nested values are outside the fixed schema and remain bounded inputs.
#[test]
fn deeply_nested_invalid_values_are_refused() {
    for depth in [128, 256, 1024] {
        let nested_array = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
        let nested_object = format!("{}null{}", "{\"nested\":".repeat(depth), "}".repeat(depth));


        for pointer in [
            "",
            "/provenance",
            "/provenance/source",
            "/tableau/0/rank",
            "/stock",
        ] {
            for nested in [&nested_array, &nested_object] {
                let bytes = raw_replacement(pointer, nested);
                assert!(bytes.len() < MAX_DOCUMENT_BYTES);
                reject(&bytes);
            }
        }
    }
}


/// Short reads are ordinary progress, never an implicit EOF or a parse boundary.
#[test]
fn stream_reader_accepts_short_reads_and_exact_cap_eof() {
    let expected = accept(FIXTURE);


    for chunk_size in [1, 2, 7, 31, 1024] {
        let mut reader = TestReader::new(FIXTURE, chunk_size);
        assert_eq!(
            read_from(&mut reader).expect("short reads are valid"),
            expected
        );
        assert_eq!(reader.position, FIXTURE.len());
        assert!(reader.calls > 1);
    }
    let bytes = padded_document(MAX_DOCUMENT_BYTES);
    let mut reader = TestReader::new(&bytes, 97);
    assert_eq!(
        read_from(&mut reader).expect("exact cap with EOF"),
        expected
    );
    assert_eq!(reader.position, MAX_DOCUMENT_BYTES);
}


/// Interrupted reads retry without losing bytes or consuming the finite input twice.
#[test]
fn stream_reader_retries_finite_interrupted_reads() {
    let mut reader = TestReader::new(FIXTURE, 11);
    reader.interruptions_left = 4;
    assert_eq!(
        read_from(&mut reader).expect("Interrupted is retried"),
        accept(FIXTURE)
    );
    assert_eq!(reader.interruptions_left, 0);
    assert_eq!(reader.position, FIXTURE.len());
}


/// Errors before, within and after a complete document cannot become success.
#[test]
fn stream_read_errors_propagate_even_when_a_valid_document_precedes_them() {
    for offset in [0, 1, FIXTURE.len() / 2, FIXTURE.len()] {
        let mut reader = TestReader::new(FIXTURE, 17);
        reader.fail_at = Some(offset);
        let error = read_from(&mut reader).expect_err("read failure is not EOF");
        assert_error_sane(&error);
        assert_eq!(reader.position, offset);
    }
}


/// A valid prefix cannot cause acceptance or an unbounded drain of oversized input.
#[test]
fn oversized_stream_consumes_only_the_single_extra_byte_probe() {
    let bytes = padded_document(MAX_DOCUMENT_BYTES + 512);


    for chunk_size in [1, 17, 4096, MAX_DOCUMENT_BYTES + 512] {
        let mut reader = TestReader::new(&bytes, chunk_size);
        let error = read_from(&mut reader).expect_err("oversized stream is refused");
        assert_error_sane(&error);
        assert_eq!(reader.position, MAX_DOCUMENT_BYTES + 1);
    }
}


/// A simulated infinite stream refuses within a deterministic finite read budget.
#[test]
fn non_eof_stream_stops_after_65537_bytes_without_another_read() {
    let mut reader = WhitespaceStream::default();
    let error = read_from(&mut reader).expect_err("non-EOF whitespace exceeds the cap");
    assert_error_sane(&error);
    assert_eq!(reader.delivered, 65_537);
}


/// Large invalid values and unknown key names cannot be reflected wholesale.
#[test]
fn hostile_document_content_cannot_expand_error_display_without_bound() {
    let hostile = "DO_NOT_ECHO_DOCUMENT_".repeat(1000);


    for pointer in [
        "/game",
        "/deal_id",
        "/provenance/source",
        "/tableau/0/rank",
        "/stock/0/suit",
    ] {
        let mut document = fixture_value();
        *document.pointer_mut(pointer).expect("known field") = json!(hostile);
        let bytes = encode(&document);
        assert!(bytes.len() < MAX_DOCUMENT_BYTES);
        reject(&bytes);
        let error = from_slice(&bytes).expect_err("hostile value is invalid");
        assert!(!error.to_string().contains(&hostile));
    }


    for pointer in OBJECT_PATHS {
        let mut document = fixture_value();
        document
            .pointer_mut(pointer)
            .expect("known object")
            .as_object_mut()
            .expect("object")
            .insert(hostile.clone(), json!(false));
        let bytes = encode(&document);
        assert!(bytes.len() < MAX_DOCUMENT_BYTES);
        reject(&bytes);
    }
}
