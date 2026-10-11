//! Public model contracts, independent of the JSON codec and its fixture reader.
//!
//! Every arrangement here is synthetic. Acceptance proves model consistency,
//! never screen recognition, restoration of a live board or permission for input.

#![cfg(feature = "pyramid")]

use std::collections::HashSet;
use std::error::Error;
use xsar::games::cards::{Card, Rank, Suit};
use xsar::games::pyramid::{
    CardLocation, CompleteDeal, DealError, DealMetadata, MetadataField, MetadataProblem,
    ProvenanceKind, Restoration, MAX_DEAL_ID_BYTES, MAX_EVIDENCE_ID_BYTES, MAX_PRODUCER_BYTES,
    MAX_SOURCE_BYTES, STOCK_LEN, TABLEAU_LEN,
};


/// Independently enumerated rank identities and their exact portable spellings.
const EXPECTED_RANKS: [(Rank, &str); 13] = [
    (Rank::Ace, "A"),
    (Rank::Two, "2"),
    (Rank::Three, "3"),
    (Rank::Four, "4"),
    (Rank::Five, "5"),
    (Rank::Six, "6"),
    (Rank::Seven, "7"),
    (Rank::Eight, "8"),
    (Rank::Nine, "9"),
    (Rank::Ten, "10"),
    (Rank::Jack, "J"),
    (Rank::Queen, "Q"),
    (Rank::King, "K"),
];


/// Independently enumerated suits, without assuming enum discriminant values.
const EXPECTED_SUITS: [(Suit, &str); 4] = [
    (Suit::Clubs, "clubs"),
    (Suit::Diamonds, "diamonds"),
    (Suit::Hearts, "hearts"),
    (Suit::Spades, "spades"),
];


/// Checked fields paired with the approved byte limits, not implementation values.
const METADATA_LIMITS: [(MetadataField, usize); 4] = [
    (MetadataField::DealId, 64),
    (MetadataField::Producer, 64),
    (MetadataField::Source, 256),
    (MetadataField::EvidenceId, 128),
];


/// Supply exactly one varied field while all other metadata remain valid.
fn metadata_with(field: MetadataField, value: &str) -> Result<DealMetadata, DealError> {
    let mut deal_id = "model-fixture-1";
    let mut producer = "model tests";
    let mut source = "Synthetic standard deck";
    let mut evidence_id = None;


    match field {
        MetadataField::DealId => deal_id = value,
        MetadataField::Producer => producer = value,
        MetadataField::Source => source = value,
        MetadataField::EvidenceId => evidence_id = Some(value),
    }
    DealMetadata::try_new(
        deal_id,
        ProvenanceKind::SyntheticFixture,
        Restoration::NotApplicable,
        producer,
        source,
        evidence_id,
    )
}


/// Read back the varied field solely through the immutable public API.
fn metadata_value(metadata: &DealMetadata, field: MetadataField) -> &str {
    match field {
        MetadataField::DealId => metadata.deal_id(),
        MetadataField::Producer => metadata.producer(),
        MetadataField::Source => metadata.source(),
        MetadataField::EvidenceId => metadata.evidence_id().expect("supplied evidence reference"),
    }
}


/// Create known fixture provenance without claiming any live restoration.
fn fixture_metadata() -> DealMetadata {
    metadata_with(MetadataField::DealId, "model-fixture-1").expect("valid fixture metadata")
}


/// Build all standard identities without using the model's enumeration constants.
fn standard_deck() -> [Card; 52] {
    std::array::from_fn(|index| {
        Card::new(EXPECTED_RANKS[index % 13].0, EXPECTED_SUITS[index / 13].0)
    })
}


/// Pass the prescribed fixed 28/24 arrays through the public checked constructor.
fn deal_from(deck: [Card; 52]) -> Result<CompleteDeal, DealError> {
    let tableau: [Card; 28] = deck[..28].try_into().expect("28 known tableau cards");
    let stock: [Card; 24] = deck[28..].try_into().expect("24 known stock cards");
    CompleteDeal::try_new(tableau, stock, fixture_metadata())
}


/// Translate a known combined-array index into its expected diagnostic location.
fn expected_location(index: usize) -> CardLocation {
    assert!(index < 52);


    if index < 28 {
        CardLocation::Tableau { index }
    } else {
        CardLocation::Stock {
            ordinal: index - 28,
        }
    }
}


/// Replace one identity and assert both exact occurrences of the resulting duplicate.
fn assert_duplicate(source: usize, destination: usize) {
    let mut deck = standard_deck();
    let repeated = deck[source];
    deck[destination] = repeated;
    assert_eq!(
        deal_from(deck),
        Err(DealError::DuplicateCard {
            card: repeated,
            first: expected_location(source.min(destination)),
            second: expected_location(source.max(destination)),
        }),
        "copy from {source} to {destination}",
    );
}


/// All public ranks are covered by an exhaustive match, with no unknown variant.
#[test]
fn rank_domain_and_spellings_are_exact() {
    assert_eq!(Rank::ALL, EXPECTED_RANKS.map(|(rank, _)| rank));


    for (rank, spelling) in EXPECTED_RANKS {
        let expected = match rank {
            Rank::Ace => "A",
            Rank::Two => "2",
            Rank::Three => "3",
            Rank::Four => "4",
            Rank::Five => "5",
            Rank::Six => "6",
            Rank::Seven => "7",
            Rank::Eight => "8",
            Rank::Nine => "9",
            Rank::Ten => "10",
            Rank::Jack => "J",
            Rank::Queen => "Q",
            Rank::King => "K",
        };
        assert_eq!(rank.as_str(), expected);
        assert_eq!(rank.to_string(), spelling);
    }
}


/// Explicit numeric mappings are checked in both directions without enum casts.
#[test]
fn rank_numeric_values_cover_all_thirteen_ranks_and_preserve_all_suits() {
    let mappings = [
        (Rank::Ace, 1),
        (Rank::Two, 2),
        (Rank::Three, 3),
        (Rank::Four, 4),
        (Rank::Five, 5),
        (Rank::Six, 6),
        (Rank::Seven, 7),
        (Rank::Eight, 8),
        (Rank::Nine, 9),
        (Rank::Ten, 10),
        (Rank::Jack, 11),
        (Rank::Queen, 12),
        (Rank::King, 13),
    ];
    let mut identities = HashSet::new();


    for (rank, value) in mappings {
        assert_eq!(rank.value(), value);
        assert_eq!(Rank::from_value(value), Some(rank));


        for (suit, _) in EXPECTED_SUITS {
            let recovered = Rank::from_value(value).expect("known numeric rank");
            let card = Card::new(recovered, suit);
            assert_eq!(card, Card::new(rank, suit));
            assert_eq!(card.rank().value(), value);
            assert_eq!(card.suit(), suit);
            assert!(identities.insert(card));
        }
    }
    assert_eq!(identities.len(), 52);
}


/// All public suits are covered by an exhaustive match, including spelling/case.
#[test]
fn suit_domain_and_spellings_are_exact() {
    assert_eq!(Suit::ALL, EXPECTED_SUITS.map(|(suit, _)| suit));


    for (suit, spelling) in EXPECTED_SUITS {
        let expected = match suit {
            Suit::Clubs => "clubs",
            Suit::Diamonds => "diamonds",
            Suit::Hearts => "hearts",
            Suit::Spades => "spades",
        };
        assert_eq!(suit.as_str(), expected);
        assert_eq!(suit.to_string(), spelling);
    }
}


/// Rank and suit together define exactly 52 equality/hash identities.
#[test]
fn card_identity_equality_hash_and_display_cover_the_complete_domain() {
    let mut identities = HashSet::new();


    for (suit, suit_name) in EXPECTED_SUITS {
        for (rank, rank_name) in EXPECTED_RANKS {
            let card = Card::new(rank, suit);
            assert_eq!(card.rank(), rank);
            assert_eq!(card.suit(), suit);
            assert_eq!(card.to_string(), format!("{rank_name} of {suit_name}"));
            assert_eq!(card, Card::new(rank, suit));
            assert!(identities.insert(card));
            assert!(!identities.insert(Card::new(rank, suit)));
        }
    }
    assert_eq!(identities.len(), 52);
    assert_ne!(
        Card::new(Rank::Ace, Suit::Clubs),
        Card::new(Rank::Two, Suit::Clubs)
    );
    assert_ne!(
        Card::new(Rank::Ace, Suit::Clubs),
        Card::new(Rank::Ace, Suit::Spades)
    );
}


/// Card values and their accessors remain usable in const consumer declarations.
#[test]
fn card_construction_and_accessors_are_const_compatible() {
    const CARD: Card = Card::new(Rank::King, Suit::Spades);


    const RANK: Rank = CARD.rank();


    const SUIT: Suit = CARD.suit();


    const RANK_NAME: &str = RANK.as_str();


    const SUIT_NAME: &str = SUIT.as_str();

    assert_eq!(CARD, Card::new(Rank::King, Suit::Spades));
    assert_eq!((RANK_NAME, SUIT_NAME), ("K", "spades"));
}


/// Explicit row expectations do not repeat the production triangular-index formula.
#[test]
fn every_tableau_row_and_column_has_the_prescribed_identity() {
    use Rank::{Ace, Eight, Five, Four, Jack, King, Nine, Queen, Seven, Six, Ten, Three, Two};
    use Suit::{Clubs, Diamonds, Hearts};

    let rows: [&[(Rank, Suit)]; 7] = [
        &[(Ace, Clubs)],
        &[(Two, Clubs), (Three, Clubs)],
        &[(Four, Clubs), (Five, Clubs), (Six, Clubs)],
        &[(Seven, Clubs), (Eight, Clubs), (Nine, Clubs), (Ten, Clubs)],
        &[
            (Jack, Clubs),
            (Queen, Clubs),
            (King, Clubs),
            (Ace, Diamonds),
            (Two, Diamonds),
        ],
        &[
            (Three, Diamonds),
            (Four, Diamonds),
            (Five, Diamonds),
            (Six, Diamonds),
            (Seven, Diamonds),
            (Eight, Diamonds),
        ],
        &[
            (Nine, Diamonds),
            (Ten, Diamonds),
            (Jack, Diamonds),
            (Queen, Diamonds),
            (King, Diamonds),
            (Ace, Hearts),
            (Two, Hearts),
        ],
    ];
    let deal = deal_from(standard_deck()).expect("complete synthetic deal");
    let tableau: &[Card; 28] = deal.tableau();
    let mut visited = 0;
    assert_eq!(TABLEAU_LEN, 28);


    for (row, cards) in rows.into_iter().enumerate() {
        assert_eq!(cards.len(), row + 1);


        for (column, &(rank, suit)) in cards.iter().enumerate() {
            let expected = Card::new(rank, suit);
            assert_eq!(
                deal.card_at(row, column),
                Ok(expected),
                "row {row}, column {column}"
            );
            assert_eq!(tableau[visited], expected, "tableau index {visited}");
            visited += 1;
        }
    }
    assert_eq!(visited, 28);
}


/// Stock expectations explicitly include the initially visible card at ordinal zero.
#[test]
fn every_stock_ordinal_remains_in_first_draw_order() {
    use Rank::{Ace, Eight, Five, Four, Jack, King, Nine, Queen, Seven, Six, Ten, Three, Two};
    use Suit::{Hearts, Spades};

    let expected = [
        (Three, Hearts),
        (Four, Hearts),
        (Five, Hearts),
        (Six, Hearts),
        (Seven, Hearts),
        (Eight, Hearts),
        (Nine, Hearts),
        (Ten, Hearts),
        (Jack, Hearts),
        (Queen, Hearts),
        (King, Hearts),
        (Ace, Spades),
        (Two, Spades),
        (Three, Spades),
        (Four, Spades),
        (Five, Spades),
        (Six, Spades),
        (Seven, Spades),
        (Eight, Spades),
        (Nine, Spades),
        (Ten, Spades),
        (Jack, Spades),
        (Queen, Spades),
        (King, Spades),
    ];
    let deal = deal_from(standard_deck()).expect("complete synthetic deal");
    let stock: &[Card; 24] = deal.stock();
    assert_eq!(STOCK_LEN, 24);


    for (ordinal, (rank, suit)) in expected.into_iter().enumerate() {
        assert_eq!(
            stock[ordinal],
            Card::new(rank, suit),
            "stock ordinal {ordinal}"
        );
    }
}


/// Completeness does not require sorted identities or a particular tableau/stock split.
#[test]
fn complete_deals_accept_every_identity_at_every_original_position() {
    for reversed in [false, true] {
        for rotation in 0..52 {
            let mut deck = standard_deck();


            if reversed {
                deck.reverse();
            }
            deck.rotate_left(rotation);
            let deal = deal_from(deck).expect("permutation remains a complete deck");
            assert_eq!(deal.tableau().as_slice(), &deck[..28]);
            assert_eq!(deal.stock().as_slice(), &deck[28..]);
            assert_eq!(deal.metadata(), &fixture_metadata());
        }
    }
}


/// Every distinct tableau-to-tableau replacement reports precise original locations.
#[test]
fn duplicates_within_tableau_report_the_card_and_both_indices() {
    for source in 0..28 {
        for destination in 0..28 {
            if source != destination {
                assert_duplicate(source, destination);
            }
        }
    }
}


/// Every distinct stock-to-stock replacement reports ordinals, not combined indices.
#[test]
fn duplicates_within_stock_report_the_card_and_both_ordinals() {
    for source in 28..52 {
        for destination in 28..52 {
            if source != destination {
                assert_duplicate(source, destination);
            }
        }
    }
}


/// Both directions of every cross-array duplicate retain tableau-first diagnostics.
#[test]
fn duplicates_across_arrays_report_tableau_and_stock_locations() {
    for tableau_index in 0..28 {
        for stock_index in 28..52 {
            assert_duplicate(tableau_index, stock_index);
            assert_duplicate(stock_index, tableau_index);
        }
    }
}


/// Repeated sightings cannot hide the earliest duplicate behind a later occurrence.
#[test]
fn three_occurrences_report_the_first_two_in_original_order() {
    let mut deck = standard_deck();
    deck[27] = deck[0];
    deck[51] = deck[0];
    assert_eq!(
        deal_from(deck),
        Err(DealError::DuplicateCard {
            card: Card::new(Rank::Ace, Suit::Clubs),
            first: CardLocation::Tableau { index: 0 },
            second: CardLocation::Tableau { index: 27 },
        }),
    );
}


/// Invalid rows and columns, including machine maxima, are refused before arithmetic.
#[test]
fn invalid_positions_never_overflow_or_alias_a_valid_card() {
    let deal = deal_from(standard_deck()).expect("complete synthetic deal");


    for row in [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        28,
        usize::MAX / 2,
        usize::MAX - 1,
        usize::MAX,
    ] {
        for column in [
            0,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            28,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ] {
            if row > 6 || column > row {
                assert_eq!(
                    deal.card_at(row, column),
                    Err(DealError::InvalidPosition { row, column })
                );
            }
        }
    }
}


/// Public limit constants exactly match the accepted schema-1 metadata contract.
#[test]
fn metadata_limits_are_exact_utf8_byte_limits() {
    assert_eq!(MAX_DEAL_ID_BYTES, 64);
    assert_eq!(MAX_PRODUCER_BYTES, 64);
    assert_eq!(MAX_SOURCE_BYTES, 256);
    assert_eq!(MAX_EVIDENCE_ID_BYTES, 128);
}


/// Each byte length from empty through one byte over the cap is checked separately.
#[test]
fn every_ascii_metadata_length_is_accepted_or_refused_at_the_exact_boundary() {
    for (field, maximum) in METADATA_LIMITS {
        for length in 0..=maximum + 1 {
            let input = "a".repeat(length);
            let result = metadata_with(field, &input);


            if (1..=maximum).contains(&length) {
                let metadata = result.expect("in-bound ASCII metadata");
                assert_eq!(metadata_value(&metadata, field), input);
            } else {
                assert_eq!(
                    result,
                    Err(DealError::InvalidMetadata {
                        field,
                        problem: MetadataProblem::Length
                    }),
                    "field {field:?}, length {length}",
                );
            }
        }
    }
}


/// Two-, three- and four-byte scalar values obey byte caps, including mixed endings.
#[test]
fn utf8_metadata_lengths_are_measured_in_bytes_without_truncation() {
    for (field, maximum) in [(MetadataField::Producer, 64), (MetadataField::Source, 256)] {
        for scalar in ["é", "€", "🂡"] {
            for length in 1..=maximum + scalar.len() {
                let input = format!(
                    "{}{}",
                    scalar.repeat(length / scalar.len()),
                    "x".repeat(length % scalar.len())
                );
                assert_eq!(input.len(), length);
                let result = metadata_with(field, &input);


                if length <= maximum {
                    let metadata = result.expect("in-bound UTF-8 metadata");
                    assert_eq!(metadata_value(&metadata, field), input);
                } else {
                    assert_eq!(
                        result,
                        Err(DealError::InvalidMetadata {
                            field,
                            problem: MetadataProblem::Length
                        })
                    );
                }
            }
        }
    }
}


/// Every ASCII character is independently classified against the exact token alphabet.
#[test]
fn deal_and_evidence_ids_accept_only_the_approved_ascii_alphabet() {
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";


    for field in [MetadataField::DealId, MetadataField::EvidenceId] {
        for byte in 0..=127_u8 {
            let character = char::from(byte);
            let input = format!("a{character}z");
            let result = metadata_with(field, &input);


            if alphabet.contains(character) {
                assert_eq!(
                    metadata_value(&result.expect("approved token character"), field),
                    input
                );
            } else {
                assert_eq!(
                    result,
                    Err(DealError::InvalidMetadata {
                        field,
                        problem: MetadataProblem::Characters
                    })
                );
            }
        }
    }
}


/// Valid UTF-8 outside ASCII, including confusable letters/digits, cannot form a token.
#[test]
fn tokens_reject_non_ascii_letters_digits_punctuation_and_emoji() {
    for field in [MetadataField::DealId, MetadataField::EvidenceId] {
        for input in ["é", "中", "🂡", "Ａ", "١", "−", "a\u{0301}", "a\u{200b}b"] {
            assert_eq!(
                metadata_with(field, input),
                Err(DealError::InvalidMetadata {
                    field,
                    problem: MetadataProblem::Characters
                }),
                "field {field:?}, input {input:?}",
            );
        }
    }
}


/// Both Unicode control ranges are rejected at the beginning, middle and end of text.
#[test]
fn metadata_rejects_every_unicode_control_character() {
    for codepoint in (0..=0x1f).chain(0x7f..=0x9f) {
        let control = char::from_u32(codepoint).expect("control scalar");


        for (field, _) in METADATA_LIMITS {
            for input in [
                format!("{control}abc"),
                format!("a{control}bc"),
                format!("abc{control}"),
            ] {
                assert_eq!(
                    metadata_with(field, &input),
                    Err(DealError::InvalidMetadata {
                        field,
                        problem: MetadataProblem::Characters
                    }),
                    "field {field:?}, control U+{codepoint:04X}",
                );
            }
        }
    }
}


/// Human-readable fields retain allowed punctuation, spaces and Unicode verbatim.
#[test]
fn text_metadata_preserves_printable_ascii_and_unicode_without_normalisation() {
    for field in [MetadataField::Producer, MetadataField::Source] {
        for byte in b' '..=b'~' {
            let input = char::from(byte).to_string();
            let metadata = metadata_with(field, &input).expect("printable ASCII text");
            assert_eq!(metadata_value(&metadata, field), input);
        }


        for input in [
            "  padded text  ",
            "   ",
            "é",
            "e\u{0301}",
            "中文",
            "🂡",
            "/not/an/opened/path",
            "quote \" slash \\",
        ] {
            let metadata = metadata_with(field, input).expect("allowed human-readable text");
            assert_eq!(metadata_value(&metadata, field), input);
        }
    }
}


/// Absent evidence is allowed, while an explicitly supplied empty reference is invalid.
#[test]
fn optional_evidence_distinguishes_absence_from_empty_text() {
    assert_eq!(fixture_metadata().evidence_id(), None);
    assert_eq!(
        metadata_with(MetadataField::EvidenceId, ""),
        Err(DealError::InvalidMetadata {
            field: MetadataField::EvidenceId,
            problem: MetadataProblem::Length
        }),
    );
    let metadata =
        metadata_with(MetadataField::EvidenceId, "Evidence-012_AZ").expect("opaque token");
    assert_eq!(metadata.evidence_id(), Some("Evidence-012_AZ"));
}


/// All six provenance/restoration combinations retain their distinct contracts.
#[test]
fn fixtures_and_recognition_require_their_own_restoration_declarations() {
    let cases = [
        (
            ProvenanceKind::SyntheticFixture,
            Restoration::NotApplicable,
            true,
        ),
        (
            ProvenanceKind::SyntheticFixture,
            Restoration::Verified,
            false,
        ),
        (
            ProvenanceKind::ManualFixture,
            Restoration::NotApplicable,
            true,
        ),
        (ProvenanceKind::ManualFixture, Restoration::Verified, false),
        (
            ProvenanceKind::ScreenRecognition,
            Restoration::NotApplicable,
            false,
        ),
        (
            ProvenanceKind::ScreenRecognition,
            Restoration::Verified,
            true,
        ),
    ];


    for (provenance, restoration, valid) in cases {
        for evidence_id in [None, Some("evidence-1")] {
            let result = DealMetadata::try_new(
                "deal-1",
                provenance,
                restoration,
                "model tests",
                "synthetic test case",
                evidence_id,
            );


            if valid {
                let metadata = result.expect("compatible declaration");
                assert_eq!(metadata.provenance(), provenance);
                assert_eq!(metadata.restoration(), restoration);
                assert_eq!(metadata.evidence_id(), evidence_id);
                let deck = standard_deck();
                let deal = CompleteDeal::try_new(
                    deck[..28].try_into().expect("28 cards"),
                    deck[28..].try_into().expect("24 cards"),
                    metadata.clone(),
                )
                .expect("compatible metadata remains accepted by the complete-deal model");
                assert_eq!(deal.metadata(), &metadata);
            } else {
                assert_eq!(
                    result,
                    Err(DealError::IncompatibleRestoration {
                        provenance,
                        restoration
                    })
                );
            }
        }
    }
}


/// Metadata owns its checked input and immutable completed values survive caller edits.
#[test]
fn checked_metadata_owns_strings_and_completed_values_preserve_their_contents() {
    let mut deal_id = String::from("owned-deal");
    let mut producer = String::from("owned producer");
    let mut source = String::from("owned source");
    let mut evidence = String::from("owned-evidence");
    let metadata = DealMetadata::try_new(
        &deal_id,
        ProvenanceKind::ManualFixture,
        Restoration::NotApplicable,
        &producer,
        &source,
        Some(&evidence),
    )
    .expect("valid owned metadata");
    deal_id.clear();
    producer.clear();
    source.clear();
    evidence.clear();
    let mut deck = standard_deck();
    let tableau: [Card; 28] = deck[..28].try_into().expect("28 cards");
    let stock: [Card; 24] = deck[28..].try_into().expect("24 cards");
    let deal = CompleteDeal::try_new(tableau, stock, metadata.clone()).expect("complete fixture");
    deck.fill(Card::new(Rank::Ace, Suit::Clubs));
    let copied_deal = deal.clone();
    assert_eq!(copied_deal, deal);
    assert_eq!(copied_deal.tableau(), &tableau);
    assert_eq!(copied_deal.stock(), &stock);
    assert_eq!(copied_deal.metadata(), &metadata);
    assert_eq!(metadata.deal_id(), "owned-deal");
    assert_eq!(metadata.producer(), "owned producer");
    assert_eq!(metadata.source(), "owned source");
    assert_eq!(metadata.evidence_id(), Some("owned-evidence"));
}


/// Rejected private input never appears in Display, Debug or an error source chain.
#[test]
fn metadata_errors_are_bounded_and_do_not_echo_rejected_values() {
    let marker = "PRIVATE_INPUT_DO_NOT_ECHO";
    let oversized = marker.repeat(4096);


    for (field, _) in METADATA_LIMITS {
        let invalid_characters = format!("{marker}\n");


        for (input, problem) in [
            (oversized.as_str(), MetadataProblem::Length),
            (invalid_characters.as_str(), MetadataProblem::Characters),
        ] {
            let error = metadata_with(field, input).expect_err("invalid private metadata");
            assert_eq!(error, DealError::InvalidMetadata { field, problem });
            assert!(error.source().is_none());


            for diagnostic in [error.to_string(), format!("{error:?}")] {
                assert!(
                    diagnostic.len() <= 256,
                    "bounded diagnostic: {diagnostic:?}"
                );
                assert!(!diagnostic.contains(marker));
                assert!(!diagnostic.chars().any(char::is_control));
                assert!(diagnostic.contains(&format!("{field:?}")));
                assert!(diagnostic.contains(&format!("{problem:?}")));
            }
        }
    }
}


/// Remaining model errors expose useful typed context with bounded safe diagnostics.
#[test]
fn position_duplicate_and_restoration_errors_are_bounded_and_descriptive() {
    let duplicate = DealError::DuplicateCard {
        card: Card::new(Rank::King, Suit::Spades),
        first: CardLocation::Tableau { index: 27 },
        second: CardLocation::Stock { ordinal: 23 },
    };
    let position = DealError::InvalidPosition {
        row: usize::MAX,
        column: usize::MAX,
    };
    let restoration = DealError::IncompatibleRestoration {
        provenance: ProvenanceKind::ScreenRecognition,
        restoration: Restoration::NotApplicable,
    };
    assert_eq!(
        CardLocation::Tableau { index: 27 }.to_string(),
        "tableau index 27"
    );
    assert_eq!(
        CardLocation::Stock { ordinal: 23 }.to_string(),
        "stock ordinal 23"
    );
    assert!(duplicate.to_string().contains("K of spades"));
    assert!(duplicate.to_string().contains("tableau index 27"));
    assert!(duplicate.to_string().contains("stock ordinal 23"));
    assert!(position.to_string().contains(&usize::MAX.to_string()));
    assert!(restoration.to_string().contains("ScreenRecognition"));
    assert!(restoration.to_string().contains("NotApplicable"));


    for error in [duplicate, position, restoration] {
        assert!(error.source().is_none());


        for diagnostic in [error.to_string(), format!("{error:?}")] {
            assert!(diagnostic.len() <= 256);
            assert!(!diagnostic.chars().any(char::is_control));
        }
    }
}
