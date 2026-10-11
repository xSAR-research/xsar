//! Bounded schema-1/2 reading and schema-2 writing for complete Pyramid deals.
//!
//! Requires only `pyramid_json` and its model/serde dependencies: no QMP, image
//! or GUI features. All root/provenance/card values must be objects; unknown
//! and duplicate fields, wrong array lengths, invalid identities and incomplete
//! arrangements are refused. Optional evidence may be absent, but not null.
//! Schema 1 requires rank/suit records. Schema 2 requires card/value/suit records
//! with the exact display label and numeric rank agreeing. Versions cannot mix
//! record shapes. Reading either version yields the same typed rank/suit model;
//! writing always emits schema 2 without changing order or capture provenance.
//!
//! Input is capped before parsing at 65,536 bytes. Streams consume at most one
//! additional byte to detect oversize. Arrays have fixed lengths, metadata has
//! independent byte limits, and parser recursion protection remains enabled.
//! Parser scratch/owned strings are bounded by the document cap, not merely
//! the smaller metadata limits. This is not an exact RSS or wall-clock bound;
//! a caller-supplied blocking reader can block indefinitely.
//! Codec-generated diagnostics and top-level displays are bounded. Retained
//! caller-supplied I/O causes may contain large or sensitive text when inspected
//! through `Debug` or an error chain; callers control presentation of those causes.
//!
//! [`to_vec`] returns pretty UTF-8 bytes with a trailing newline. [`write_new`]
//! adds Unix-only, same-directory, no-clobber filesystem publication with an
//! explicit publication status on failure. Path choice and partial diagnostics
//! remain caller policy. A restoration declaration is not authenticated live
//! evidence and never authorises capture, input, restoration or route execution.

mod file;

pub use file::{write_new, CleanupFailure, PublicationStatus, PublishError, PublishStage};

use crate::games::{
    cards::{Card, Rank, Suit},
    pyramid::{
        CardLocation, CompleteDeal, DealError, DealMetadata, ProvenanceKind, Restoration,
        STOCK_LEN, TABLEAU_LEN,
    },
};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    fmt,
    io::{self, Read, Write},
    marker::PhantomData,
};


/// Maximum complete document length, including whitespace and trailing newline.
pub const MAX_DOCUMENT_BYTES: usize = 65_536;


/// The version emitted by writers; readers explicitly support versions 1 and 2.
pub const SCHEMA_VERSION: u64 = 2;


/// Typed failures without retaining untrusted JSON strings; I/O causes are retained.
#[derive(Debug)]
pub enum JsonError {
    /// A stream read failed; its original error is available through `source()`.
    Io(io::Error),
    /// Input exceeds [`MAX_DOCUMENT_BYTES`], checked before parsing.
    InputTooLarge,
    /// Encoding would exceed [`MAX_DOCUMENT_BYTES`]; no document is returned.
    OutputTooLarge,
    /// JSON syntax or required wire shape/type is invalid.
    Malformed {
        /// One-based line supplied by the JSON parser.
        line: usize,
        /// Column supplied by the JSON parser.
        column: usize,
    },
    /// The integer schema version is not supported.
    UnsupportedSchema {
        /// Refused numeric schema identifier.
        version: u64,
    },
    /// The game discriminator is not exactly `pyramid`.
    UnsupportedGame,
    /// Partial diagnostic inventory is not completed planning input.
    Incomplete,
    /// A card identity, label or version-specific field combination is invalid.
    InvalidCard {
        /// Original logical slot containing an invalid identity.
        location: CardLocation,
        /// `rank`, `suit`, `value`, `card` or `fields`; caller text is not retained.
        component: &'static str,
    },
    /// A provenance kind or restoration label is unsupported.
    InvalidProvenance,
    /// Identity uniqueness or checked metadata validation failed.
    InvalidDeal(DealError),
    /// Internal serialisation failed without exposing an incomplete byte buffer.
    Encoding,
}


impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => formatter.write_str("Pyramid JSON input I/O failed"),
            Self::InputTooLarge => write!(
                formatter,
                "Pyramid JSON input exceeds {MAX_DOCUMENT_BYTES} bytes"
            ),
            Self::OutputTooLarge => write!(
                formatter,
                "Pyramid JSON output exceeds {MAX_DOCUMENT_BYTES} bytes"
            ),
            Self::Malformed { line, column } => write!(
                formatter,
                "invalid Pyramid JSON syntax or schema at line {line}, column {column}"
            ),
            Self::UnsupportedSchema { version } => {
                write!(formatter, "unsupported Pyramid schema version {version}")
            }
            Self::UnsupportedGame => formatter.write_str("JSON game must be pyramid"),
            Self::Incomplete => formatter.write_str("Pyramid JSON must describe a complete deal"),
            Self::InvalidCard {
                location,
                component,
            } => write!(formatter, "invalid card {component} at {location}"),
            Self::InvalidProvenance => {
                formatter.write_str("unsupported Pyramid provenance or restoration label")
            }
            Self::InvalidDeal(error) => write!(formatter, "invalid complete Pyramid deal: {error}"),
            Self::Encoding => formatter.write_str("could not encode complete Pyramid JSON"),
        }
    }
}


impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidDeal(error) => Some(error),
            _ => None,
        }
    }
}


/// Parse exactly one bounded complete deal, rejecting non-whitespace trailing data.
///
/// No files are opened. Unknown/duplicate fields, unsupported schemas/games,
/// invalid identities, mismatched labels/values and incomplete or incompatible
/// restoration declarations return errors. Schema 1 rank/suit records and schema
/// 2 card/value/suit records are validated separately, regardless of object-key
/// order. JSON declarations do not prove recognition or restoration.
///
/// ```
/// use xsar::pyramid_json::{from_slice, JsonError, MAX_DOCUMENT_BYTES};
/// let oversized = vec![b' '; MAX_DOCUMENT_BYTES + 1];
/// assert!(matches!(from_slice(&oversized), Err(JsonError::InputTooLarge)));
/// ```
pub fn from_slice(bytes: &[u8]) -> Result<CompleteDeal, JsonError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(JsonError::InputTooLarge);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let wire = Object::<WireDeal>::deserialize(&mut deserializer).map_err(malformed)?;
    deserializer.end().map_err(malformed)?;
    wire.0.into_deal()
}


/// Read at most 65,537 bytes, then apply [`from_slice`]'s complete-deal checks.
///
/// Interrupted reads are retried; other I/O errors are preserved as causes.
/// The caller owns the reader, stream selection and cancellation. No blocking
/// deadline is promised and file metadata is not trusted as an input-size bound.
pub fn read_from(reader: impl Read) -> Result<CompleteDeal, JsonError> {
    let mut bounded = reader.take((MAX_DOCUMENT_BYTES + 1) as u64);
    let mut bytes = Vec::with_capacity(MAX_DOCUMENT_BYTES + 1);
    bounded.read_to_end(&mut bytes).map_err(JsonError::Io)?;
    from_slice(&bytes)
}


/// Encode a validated immutable deal as pretty schema-2 JSON plus one newline.
///
/// Field and card order are deterministic. The output buffer has a fixed byte
/// ceiling; a failed encoding returns no partial document. This function does
/// not create, replace, sync or atomically publish any filesystem destination.
/// Reading a schema-1 document and writing its model upgrades representation
/// only: identity, order, deal metadata and acquisition provenance are preserved.
pub fn to_vec(deal: &CompleteDeal) -> Result<Vec<u8>, JsonError> {
    let wire = WireDeal::from_deal(deal);
    let mut output = BoundedOutput {
        bytes: Vec::with_capacity(MAX_DOCUMENT_BYTES),
        exceeded: false,
    };


    if serde_json::to_writer_pretty(&mut output, &wire).is_err() {
        return Err(if output.exceeded {
            JsonError::OutputTooLarge
        } else {
            JsonError::Encoding
        });
    }
    output
        .write_all(b"\n")
        .map_err(|_| JsonError::OutputTooLarge)?;
    Ok(output.bytes)
}


/// Discard potentially attacker-sized parser messages while retaining location.
fn malformed(error: serde_json::Error) -> JsonError {
    JsonError::Malformed {
        line: error.line(),
        column: error.column(),
    }
}


/// Private fixed-capacity logical output with explicit overflow signalling.
struct BoundedOutput {
    /// Complete bytes accumulated so far, never returned on error.
    bytes: Vec<u8>,
    /// Distinguishes our byte ceiling from another serialisation failure.
    exceeded: bool,
}


impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_DOCUMENT_BYTES - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("Pyramid JSON output byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }


    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}


/// Require an object instead of accepting Serde's positional struct-array form.
#[derive(Serialize)]
#[serde(transparent)]
struct Object<Value>(Value);


impl<'de, Value: Deserialize<'de>> Deserialize<'de> for Object<Value> {
    fn deserialize<Decoder: Deserializer<'de>>(decoder: Decoder) -> Result<Self, Decoder::Error> {
        decoder.deserialize_map(ObjectVisitor(PhantomData))
    }
}


/// Map-only delegation retains derive's required/duplicate/unknown-field checks.
struct ObjectVisitor<Value>(PhantomData<Value>);


impl<'de, Value: Deserialize<'de>> Visitor<'de> for ObjectVisitor<Value> {
    type Value = Object<Value>;


    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }


    fn visit_map<Access: MapAccess<'de>>(
        self,
        access: Access,
    ) -> Result<Self::Value, Access::Error> {
        Value::deserialize(serde::de::value::MapAccessDeserializer::new(access)).map(Object)
    }
}


/// Private fixed wire shape; no unchecked public deal is deserialised directly.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireDeal {
    /// Explicit supported numeric schema identifier.
    schema_version: u64,
    /// Exact lower-case game discriminator.
    game: String,
    /// Label later checked by the model before its owned copy.
    deal_id: String,
    /// Must be true; partial inventories cannot be planning input.
    complete: bool,
    /// Origin-compatible restoration declaration.
    restoration: String,
    /// Fixed object containing bounded source declarations.
    provenance: Object<WireProvenance>,
    /// Exactly 28 original tableau identities in canonical row order.
    tableau: [Object<WireCard>; TABLEAU_LEN],
    /// Exactly 24 original stock identities in first-draw order.
    stock: [Object<WireCard>; STOCK_LEN],
}


/// Private provenance object; explicit null evidence is not an absent field.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireProvenance {
    /// Synthetic, manual or screen-recognition origin label.
    kind: String,
    /// Bounded producer text, checked during model construction.
    producer: String,
    /// Bounded source text, never interpreted as a filesystem path.
    source: String,
    /// Optional token; omission is valid, explicit null is refused.
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    evidence_id: Option<String>,
}


/// Deserialize a present optional field as a string, rejecting explicit null.
fn present_string<'de, Decoder: Deserializer<'de>>(
    decoder: Decoder,
) -> Result<Option<String>, Decoder::Error> {
    String::deserialize(decoder).map(Some)
}


/// Deserialize a present numeric value as an integer byte, rejecting null or floats.
fn present_value<'de, Decoder: Deserializer<'de>>(
    decoder: Decoder,
) -> Result<Option<u8>, Decoder::Error> {
    u8::deserialize(decoder).map(Some)
}


/// Bounded union of card fields, validated against the explicit root version.
///
/// Optional storage permits root fields in any order, not optional schema fields.
/// All present fields reject null and derive retains duplicate/unknown-key checks.
/// Writers construct only card/value/suit, never legacy or mixed records.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireCard {
    /// Schema-2 canonical display label, checked against the typed identity.
    #[serde(default, deserialize_with = "present_string", skip_serializing_if = "Option::is_none")]
    card: Option<String>,
    /// Schema-2 integer rank in 1..=13; semantic bounds follow parsing.
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    value: Option<u8>,
    /// Exact lower-case clubs/diamonds/hearts/spades suit string.
    suit: String,
    /// Required only for schema 1; forbidden in schema 2 and never written.
    #[serde(default, deserialize_with = "present_string", skip_serializing_if = "Option::is_none")]
    rank: Option<String>,
}


impl WireCard {
    /// Require the exact version-specific shape and one consistent canonical identity.
    fn card(&self, schema_version: u64, location: CardLocation) -> Result<Card, JsonError> {
        let rank = match (schema_version, self.rank.as_deref(), self.card.as_deref(), self.value) {
            (1, Some(rank), None, None) => Rank::ALL
                .into_iter()
                .find(|candidate| candidate.as_str() == rank)
                .ok_or(JsonError::InvalidCard { location, component: "rank" })?,
            (SCHEMA_VERSION, None, Some(_), Some(value)) => Rank::from_value(value)
                .ok_or(JsonError::InvalidCard { location, component: "value" })?,
            _ => return Err(JsonError::InvalidCard { location, component: "fields" }),
        };
        let suit = Suit::ALL
            .into_iter()
            .find(|suit| suit.as_str() == self.suit)
            .ok_or(JsonError::InvalidCard {
                location,
                component: "suit",
            })?;
        let card = Card::new(rank, suit);


        if self.card.as_deref().is_some_and(|label| label != card.to_string()) {
            return Err(JsonError::InvalidCard { location, component: "card" });
        }
        Ok(card)
    }


    /// Encode a valid closed-domain identity without alternative spellings.
    fn from_card(card: Card) -> Object<Self> {
        Object(Self {
            card: Some(card.to_string()),
            value: Some(card.rank().value()),
            suit: card.suit().as_str().to_owned(),
            rank: None,
        })
    }
}


impl WireDeal {
    /// Validate declarations, identities and completeness before releasing a model.
    fn into_deal(self) -> Result<CompleteDeal, JsonError> {
        if !matches!(self.schema_version, 1 | SCHEMA_VERSION) {
            return Err(JsonError::UnsupportedSchema {
                version: self.schema_version,
            });
        }


        if self.game != "pyramid" {
            return Err(JsonError::UnsupportedGame);
        }


        if !self.complete {
            return Err(JsonError::Incomplete);
        }
        let provenance = match self.provenance.0.kind.as_str() {
            "synthetic_fixture" => ProvenanceKind::SyntheticFixture,
            "manual_fixture" => ProvenanceKind::ManualFixture,
            "screen_recognition" => ProvenanceKind::ScreenRecognition,
            _ => return Err(JsonError::InvalidProvenance),
        };
        let restoration = match self.restoration.as_str() {
            "verified" => Restoration::Verified,
            "not_applicable" => Restoration::NotApplicable,
            _ => return Err(JsonError::InvalidProvenance),
        };
        let metadata = DealMetadata::try_new(
            &self.deal_id,
            provenance,
            restoration,
            &self.provenance.0.producer,
            &self.provenance.0.source,
            self.provenance.0.evidence_id.as_deref(),
        )
        .map_err(JsonError::InvalidDeal)?;
        let mut tableau = [Card::new(Rank::Ace, Suit::Clubs); TABLEAU_LEN];
        let mut stock = [Card::new(Rank::Ace, Suit::Clubs); STOCK_LEN];


        for (index, wire) in self.tableau.iter().enumerate() {
            tableau[index] = wire.0.card(self.schema_version, CardLocation::Tableau { index })?;
        }


        for (ordinal, wire) in self.stock.iter().enumerate() {
            stock[ordinal] = wire.0.card(self.schema_version, CardLocation::Stock { ordinal })?;
        }
        CompleteDeal::try_new(tableau, stock, metadata).map_err(JsonError::InvalidDeal)
    }


    /// Stable field and array order for the validated public model.
    fn from_deal(deal: &CompleteDeal) -> Self {
        let metadata = deal.metadata();
        let kind = match metadata.provenance() {
            ProvenanceKind::SyntheticFixture => "synthetic_fixture",
            ProvenanceKind::ManualFixture => "manual_fixture",
            ProvenanceKind::ScreenRecognition => "screen_recognition",
        };
        let restoration = match metadata.restoration() {
            Restoration::Verified => "verified",
            Restoration::NotApplicable => "not_applicable",
        };
        Self {
            schema_version: SCHEMA_VERSION,
            game: "pyramid".to_owned(),
            deal_id: metadata.deal_id().to_owned(),
            complete: true,
            restoration: restoration.to_owned(),
            provenance: Object(WireProvenance {
                kind: kind.to_owned(),
                producer: metadata.producer().to_owned(),
                source: metadata.source().to_owned(),
                evidence_id: metadata.evidence_id().map(str::to_owned),
            }),
            tableau: deal.tableau().map(WireCard::from_card),
            stock: deal.stock().map(WireCard::from_card),
        }
    }
}


#[cfg(test)]
mod tests {
    //! Internal output-boundary checks unreachable through valid small models.

    use super::*;


    /// The byte ceiling accepts its exact bound and refuses without partial write.
    #[test]
    fn output_limit_is_exact_and_keeps_failed_writes_private() {
        let mut output = BoundedOutput {
            bytes: Vec::with_capacity(MAX_DOCUMENT_BYTES),
            exceeded: false,
        };
        output.write_all(&vec![b' '; MAX_DOCUMENT_BYTES]).unwrap();
        assert_eq!(output.bytes.len(), MAX_DOCUMENT_BYTES);
        assert!(!output.exceeded);
        assert!(output.write_all(b"\n").is_err());
        assert!(output.exceeded);
        assert_eq!(output.bytes.len(), MAX_DOCUMENT_BYTES);
        assert!(output.flush().is_ok());
    }
}
