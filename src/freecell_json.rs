//! Strict bounded FreeCell schema-1 codec; declarations grant no live authority.
use crate::games::{
    cards::{Card, Rank, Suit},
    freecell::{BoardState, CascadeStack, InitialDeal, Metadata, ModelError},
};
pub use crate::json_publication::{CleanupFailure, PublicationStatus, PublishStage};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::{
    fmt,
    io::{self, Read, Write},
    marker::PhantomData,
    path::Path,
};

/// Independent FreeCell schema version.
pub const SCHEMA_VERSION: u64 = 1;
/// Hard input/output byte ceiling.
pub const MAX_DOCUMENT_BYTES: usize = 65_536;

/// Bounded parse, storage or encoding failure.
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    /// Input exceeds the byte ceiling.
    #[error("FreeCell JSON exceeds input limit")]
    InputTooLarge,
    /// Output exceeds the byte ceiling.
    #[error("FreeCell JSON exceeds output limit")]
    OutputTooLarge,
    /// Parser diagnostics omit attacker-controlled text.
    #[error("Malformed FreeCell JSON at {line}:{column}")]
    Malformed {
        /// One-based source line.
        line: usize,
        /// One-based source column.
        column: usize,
    },
    /// Wrong closed game/schema or card labels.
    #[error("Unsupported FreeCell JSON identity or schema")]
    Unsupported,
    /// Checked physical model refused the inventory or metadata.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// Original reader error.
    #[error("FreeCell JSON read failed")]
    Io(#[source] io::Error),
    /// Serialisation failed without returning partial output.
    #[error("FreeCell JSON encoding failed")]
    Encoding,
}

/// Decode one complete document with no trailing content.
///
/// # Errors
/// Rejects oversized, duplicate/unknown/missing fields and invalid initial deals.
pub fn from_slice(bytes: &[u8]) -> Result<InitialDeal, JsonError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(JsonError::InputTooLarge);
    }
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let wire = Object::<WireDeal>::deserialize(&mut parser).map_err(malformed)?;
    parser.end().map_err(malformed)?;
    wire.0.into_deal()
}

/// Read at most65,537bytes; no filesystem path or metadata is trusted.
///
/// # Errors
/// Returns original I/O failures or the same checks as from_slice.
pub fn read_from(reader: impl Read) -> Result<InitialDeal, JsonError> {
    let mut bytes = Vec::with_capacity(MAX_DOCUMENT_BYTES + 1);
    reader
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(JsonError::Io)?;
    from_slice(&bytes)
}

/// Encode a checked initial deal in original physical order.
///
/// # Errors
/// Refuses output above65,536bytes; returns no partial buffer.
pub fn to_vec(deal: &InitialDeal) -> Result<Vec<u8>, JsonError> {
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

/// Retain only the bounded parser location.
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
            return Err(io::Error::other("FreeCell JSON output byte limit exceeded"));
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



/// Required root map; fixed arrays bound semantic shape before model validation.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireDeal {
    /// Schema discriminator.
    schema: u64,
    /// Closed game discriminator.
    game: String,
    /// Physical columns.
    cascades: [Vec<Object<WireCard>>; 8],
    /// Four required explicit empty values.
    cells: [(); 4],
    /// Suit-keyed progress.
    foundations: Object<WireFoundations>,
    /// Descriptive bounded labels.
    provenance: Object<WireMetadata>,
}
/// Required suit-keyed foundation map.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireFoundations {
    /// Clubs progress.
    clubs: u8,
    /// Diamonds progress.
    diamonds: u8,
    /// Hearts progress.
    hearts: u8,
    /// Spades progress.
    spades: u8,
}
/// Descriptive labels, not paths or acquisition authority.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireMetadata {
    /// Deal label.
    deal_id: String,
    /// Recognition profile.
    profile: String,
    /// Acquisition source label.
    source: String,
}
/// Closed rank and suit fields; unknown/duplicate keys are refused.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireCard {
    /// Canonical shared rank spelling.
    rank: String,
    /// Canonical shared suit spelling.
    suit: String,
}
impl WireCard {
    /// Resolve each printed identity without inference.
    fn card(self) -> Result<Card, JsonError> {
        let rank = Rank::ALL
            .into_iter()
            .find(|v| v.as_str() == self.rank)
            .ok_or(JsonError::Unsupported)?;
        let suit = Suit::ALL
            .into_iter()
            .find(|v| v.as_str() == self.suit)
            .ok_or(JsonError::Unsupported)?;
        Ok(Card::new(rank, suit))
    }
}
impl WireDeal {
    /// Validate fixed physical order and exact inventory.
    fn into_deal(self) -> Result<InitialDeal, JsonError> {
        if self.schema != SCHEMA_VERSION || self.game != "freecell" {
            return Err(JsonError::Unsupported);
        }
        let mut cascades: [CascadeStack; 8] = std::array::from_fn(|_| CascadeStack::default());
        for (target, column) in cascades.iter_mut().zip(self.cascades) {
            for card in column {
                target.push(card.0.card()?)?;
            }
        }
        let f = self.foundations.0;
        let m = self.provenance.0;
        let metadata = Metadata::new(m.deal_id, m.profile, m.source)?;
        let board = BoardState::new(
            cascades,
            [None; 4],
            [f.clubs, f.diamonds, f.hearts, f.spades],
        )?;
        Ok(InitialDeal::new(board, metadata)?)
    }
    /// Serialise every original physical identity, without sorting.
    fn from_deal(deal: &InitialDeal) -> Self {
        let cascades = deal.board().cascades().each_ref().map(|stack| {
            stack
                .iter()
                .map(|c| {
                    Object(WireCard {
                        rank: c.rank().as_str().to_owned(),
                        suit: c.suit().as_str().to_owned(),
                    })
                })
                .collect()
        });
        Self {
            schema: SCHEMA_VERSION,
            game: "freecell".to_owned(),
            cascades,
            cells: [(); 4],
            foundations: Object(WireFoundations {
                clubs: 0,
                diamonds: 0,
                hearts: 0,
                spades: 0,
            }),
            provenance: Object(WireMetadata {
                deal_id: deal.metadata().deal_id().to_owned(),
                profile: deal.metadata().profile().to_owned(),
                source: deal.metadata().source().to_owned(),
            }),
        }
    }
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
            Self::Encode(error) => write!(formatter, "FreeCell JSON not published: {error}"),
            Self::InvalidPath => {
                formatter.write_str("FreeCell JSON not published: invalid output filename")
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("FreeCell JSON publication requires Unix")
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
                write!(formatter, "FreeCell JSON {boundary}: failed to {stage}")?;
                if cleanup.is_some() {
                    formatter.write_str("; private temporary cleanup also failed")?;
                }
                Ok(())
            }
            Self::Published {
                cleanup,
                directory_sync,
            } => {
                formatter.write_str("FreeCell JSON published")?;
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


/// Encode before touching a path, then publish once without overwrite.
///
/// The parent hierarchy must remain trusted and stable for this blocking operation;
/// relative paths require an unchanged process working directory. Unix publication
/// uses a same-parent 0600 temporary file with at most 64 exclusive name attempts,
/// followed by file sync, no-overwrite hard link, cleanup and directory sync.
/// Other platforms refuse publication. This does not defend against hostile
/// concurrent ancestor or temporary-name replacement.
///
/// # Errors
/// Encode, publication stage and durability errors retain distinct statuses.
/// Published/Uncertain errors require inspection rather than automatic replay.
pub fn write_new(path: impl AsRef<Path>, deal: &InitialDeal) -> Result<(), PublishError> {
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
