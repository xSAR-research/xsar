//! Complete immutable original arrangements for the seven-row, single-deck profile.
//!
//! Tableau order is top-to-bottom and left-to-right; stock order is first-draw
//! order, with the initially visible LEFT card at ordinal zero. Observations,
//! live board state and native capture coordinates remain separate. Validation
//! establishes structure, not recognition accuracy or actual guest restoration.

use super::cards::Card;
use std::fmt;


/// Number of cards in the seven original tableau rows.
pub const TABLEAU_LEN: usize = 28;


/// Number of original stock cards, including the initially visible LEFT card.
pub const STOCK_LEN: usize = 24;


/// Maximum ASCII deal-label length in bytes.
pub const MAX_DEAL_ID_BYTES: usize = 64;


/// Maximum producer-label length in UTF-8 bytes.
pub const MAX_PRODUCER_BYTES: usize = 64;


/// Maximum source-description length in UTF-8 bytes.
pub const MAX_SOURCE_BYTES: usize = 256;


/// Maximum opaque ASCII evidence-reference length in bytes.
pub const MAX_EVIDENCE_ID_BYTES: usize = 128;


/// Origin declared by the producer, not independently authenticated evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceKind {
    /// Artificial known arrangement, not acquired from a running game.
    SyntheticFixture,
    /// Manually recorded known arrangement, not a live restoration claim.
    ManualFixture,
    /// Arrangement acquired by a screen reader whose caller verifies restoration.
    ScreenRecognition,
}


/// Restoration declaration; it never grants input authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Restoration {
    /// The acquiring controller declares that it verified the original live state.
    Verified,
    /// No live board restoration applies to this fixture.
    NotApplicable,
}


/// Portable location in the immutable original arrangement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardLocation {
    /// Canonical top-to-bottom, left-to-right tableau array location.
    Tableau {
        /// Zero-based array index in 0..28.
        index: usize,
    },
    /// First-draw stock order, not current live pile position.
    Stock {
        /// Zero-based ordinal in 0..24.
        ordinal: usize,
    },
}


impl fmt::Display for CardLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tableau { index } => write!(formatter, "tableau index {index}"),
            Self::Stock { ordinal } => write!(formatter, "stock ordinal {ordinal}"),
        }
    }
}


/// Checked metadata field, without retaining an invalid input string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataField {
    /// Caller-assigned ASCII deal label.
    DealId,
    /// Producer's UTF-8 label.
    Producer,
    /// Source's UTF-8 description.
    Source,
    /// Optional opaque ASCII evidence reference.
    EvidenceId,
}


/// Why a bounded metadata field was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataProblem {
    /// Empty field or byte length above its declared maximum.
    Length,
    /// Control character or character outside an ASCII token's alphabet.
    Characters,
}


/// Invalid completed-deal input or logical lookup, with bounded diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DealError {
    /// A metadata string failed validation before copying into the model.
    InvalidMetadata {
        /// Field whose bounds or characters were invalid.
        field: MetadataField,
        /// Length or character rejection.
        problem: MetadataProblem,
    },
    /// Fixtures cannot claim live restoration, and screen deals must declare it.
    IncompatibleRestoration {
        /// Producer-declared acquisition kind.
        provenance: ProvenanceKind,
        /// Incompatible restoration declaration.
        restoration: Restoration,
    },
    /// A repeated identity necessarily means another standard card is missing.
    DuplicateCard {
        /// Identity occurring more than once.
        card: Card,
        /// First occurrence in canonical original order.
        first: CardLocation,
        /// Repeated occurrence in canonical original order.
        second: CardLocation,
    },
    /// Lookup is outside rows 0..6 or columns 0..row.
    InvalidPosition {
        /// Requested zero-based row.
        row: usize,
        /// Requested zero-based column.
        column: usize,
    },
}


impl fmt::Display for DealError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMetadata { field, problem } => {
                write!(formatter, "invalid {field:?} metadata: {problem:?}")
            }
            Self::IncompatibleRestoration {
                provenance,
                restoration,
            } => {
                write!(
                    formatter,
                    "{provenance:?} is incompatible with {restoration:?} restoration"
                )
            }
            Self::DuplicateCard {
                card,
                first,
                second,
            } => {
                write!(formatter, "duplicate {card}: {first} and {second}")
            }
            Self::InvalidPosition { row, column } => {
                write!(formatter, "invalid Pyramid row {row}, column {column}")
            }
        }
    }
}


impl std::error::Error for DealError {}


/// Validated bounded provenance, kept immutable once constructed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DealMetadata {
    /// ASCII label, not a global-uniqueness or cryptographic guarantee.
    deal_id: String,
    /// Declared origin of the complete arrangement.
    provenance: ProvenanceKind,
    /// Validated origin-compatible declaration, not independent proof.
    restoration: Restoration,
    /// Bounded producer label.
    producer: String,
    /// Bounded source description, never opened as a path.
    source: String,
    /// Opaque optional evidence reference, never opened as a path.
    evidence_id: Option<String>,
}


impl DealMetadata {
    /// Validate lengths, characters and provenance before copying any strings.
    ///
    /// Tokens permit only ASCII letters, digits, hyphen and underscore. Text
    /// fields reject Unicode control characters. All supplied strings are nonempty.
    /// Screen recognition requires `Verified`; fixtures require `NotApplicable`.
    /// An error does not retain or echo invalid caller-provided text.
    pub fn try_new(
        deal_id: &str,
        provenance: ProvenanceKind,
        restoration: Restoration,
        producer: &str,
        source: &str,
        evidence_id: Option<&str>,
    ) -> Result<Self, DealError> {
        validate_text(deal_id, MetadataField::DealId, MAX_DEAL_ID_BYTES, true)?;
        validate_text(producer, MetadataField::Producer, MAX_PRODUCER_BYTES, false)?;
        validate_text(source, MetadataField::Source, MAX_SOURCE_BYTES, false)?;


        if let Some(evidence) = evidence_id {
            validate_text(
                evidence,
                MetadataField::EvidenceId,
                MAX_EVIDENCE_ID_BYTES,
                true,
            )?;
        }
        let expected = match provenance {
            ProvenanceKind::ScreenRecognition => Restoration::Verified,
            ProvenanceKind::SyntheticFixture | ProvenanceKind::ManualFixture => {
                Restoration::NotApplicable
            }
        };


        if restoration != expected {
            return Err(DealError::IncompatibleRestoration {
                provenance,
                restoration,
            });
        }
        Ok(Self {
            deal_id: deal_id.to_owned(),
            provenance,
            restoration,
            producer: producer.to_owned(),
            source: source.to_owned(),
            evidence_id: evidence_id.map(str::to_owned),
        })
    }


    /// Caller-assigned deal label.
    pub fn deal_id(&self) -> &str {
        &self.deal_id
    }


    /// Producer-declared acquisition kind.
    pub fn provenance(&self) -> ProvenanceKind {
        self.provenance
    }


    /// Origin-compatible restoration declaration, not independent verification.
    pub fn restoration(&self) -> Restoration {
        self.restoration
    }


    /// Bounded producer label.
    pub fn producer(&self) -> &str {
        &self.producer
    }


    /// Bounded source description; the crate never opens it.
    pub fn source(&self) -> &str {
        &self.source
    }


    /// Optional opaque diagnostic reference; the crate never opens it.
    pub fn evidence_id(&self) -> Option<&str> {
        self.evidence_id.as_deref()
    }
}


/// Check bounded text or an ASCII token before allocating an owned model field.
fn validate_text(
    text: &str,
    field: MetadataField,
    maximum: usize,
    token: bool,
) -> Result<(), DealError> {
    if text.is_empty() || text.len() > maximum {
        return Err(DealError::InvalidMetadata {
            field,
            problem: MetadataProblem::Length,
        });
    }


    if text.chars().any(char::is_control)
        || (token
            && !text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
    {
        return Err(DealError::InvalidMetadata {
            field,
            problem: MetadataProblem::Characters,
        });
    }
    Ok(())
}


/// Exactly one standard 52-card original deal, with no partial or unknown cards.
///
/// Construction uses a fixed 52-slot identity table and linear work. No identity
/// is inferred by elimination. The model provides no mutable array/metadata access,
/// live cursor, legal-move search or permission to execute inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteDeal {
    /// Original seven rows in canonical top-to-bottom, left-to-right order.
    tableau: [Card; TABLEAU_LEN],
    /// Original first-draw order including the initially visible LEFT card.
    stock: [Card; STOCK_LEN],
    /// Checked immutable producer declarations.
    metadata: DealMetadata,
}


impl CompleteDeal {
    /// Reject duplicate identities across both fixed arrays.
    ///
    /// The closed card domain and 28+24 unique entries prove standard-deck
    /// completeness. Metadata can only be created through its checked constructor.
    pub fn try_new(
        tableau: [Card; TABLEAU_LEN],
        stock: [Card; STOCK_LEN],
        metadata: DealMetadata,
    ) -> Result<Self, DealError> {
        let mut seen = [None; TABLEAU_LEN + STOCK_LEN];


        for (index, card) in tableau.iter().chain(stock.iter()).copied().enumerate() {
            let location = if index < TABLEAU_LEN {
                CardLocation::Tableau { index }
            } else {
                CardLocation::Stock {
                    ordinal: index - TABLEAU_LEN,
                }
            };
            let identity = card.suit() as usize * 13 + card.rank() as usize;


            if let Some(first) = seen[identity] {
                return Err(DealError::DuplicateCard {
                    card,
                    first,
                    second: location,
                });
            }
            seen[identity] = Some(location);
        }
        Ok(Self {
            tableau,
            stock,
            metadata,
        })
    }


    /// Original tableau order, not the application's bottom-up CV target order.
    pub fn tableau(&self) -> &[Card; TABLEAU_LEN] {
        &self.tableau
    }


    /// Original stock order, including ordinal zero before the first advance.
    pub fn stock(&self) -> &[Card; STOCK_LEN] {
        &self.stock
    }


    /// Immutable checked declarations; not authentication or input authority.
    pub fn metadata(&self) -> &DealMetadata {
        &self.metadata
    }


    /// Look up rows 0..6 and columns 0..row, checking before index arithmetic.
    pub fn card_at(&self, row: usize, column: usize) -> Result<Card, DealError> {
        if row >= 7 || column > row {
            return Err(DealError::InvalidPosition { row, column });
        }
        Ok(self.tableau[row * (row + 1) / 2 + column])
    }
}


/// Pure planning actions, checked locations and route evidence.
#[cfg(feature = "pyramid_rules")]
pub mod planning;


/// Pure original-deal transitions; never grants guest input authority.
#[cfg(feature = "pyramid_rules")]
pub mod rules;


/// Independent explicit-pile route validation.
#[cfg(feature = "pyramid_rules")]
pub mod replay;


/// Complete-deal adapter to bounded generic search.
#[cfg(feature = "pyramid_search")]
pub mod search;
