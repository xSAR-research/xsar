//! Checked FreeCell physical storage, independent of recognition and search.
//!
//! Columns/depths are zero-based. Initial deals preserve original physical order;
//! acquisition declarations do not grant input authority.

use super::cards::{Card, Suit};

/// Pure exposed-card rules and exact physical search positions.
#[cfg(feature = "freecell_rules")]
pub mod rules;

/// Independent physical route verification, without guest input.
#[cfg(feature = "freecell_rules")]
pub mod replay;

/// Complete guided offline calculation, without configured search cutoffs.
#[cfg(feature = "freecell_search")]
pub mod search;

/// Hard per-cascade capacity, including later noninitial positions.
pub const CASCADE_CAPACITY: usize = 20;
/// Original column lengths, from left to right.
pub const INITIAL_LENGTHS: [usize; 8] = [7, 7, 7, 7, 6, 6, 6, 6];
/// Canonical foundation order; screen decorations are mapped by the application.
pub const FOUNDATION_SUITS: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

/// A stable physical location used in validation diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Location {
    /// A checked physical PLAY column/depth.
    Cascade(CascadeLocation),
    /// Zero-based physical CELL slot.
    Cell(usize),
    /// Canonical foundation suit.
    Foundation(Suit),
}

/// Checked zero-based original column and depth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CascadeLocation {
    column: usize,
    depth: usize,
}

impl CascadeLocation {
    /// Construct a physical location.
    ///
    /// # Errors
    /// Returns `Location` unless column < 8 and depth < 20.
    pub fn new(column: usize, depth: usize) -> Result<Self, ModelError> {
        if column >= 8 || depth >= CASCADE_CAPACITY {
            return Err(ModelError::Location);
        }
        Ok(Self { column, depth })
    }
    /// Original zero-based column.
    pub const fn column(self) -> usize {
        self.column
    }
    /// Original zero-based depth.
    pub const fn depth(self) -> usize {
        self.depth
    }
}

/// Checked storage and exact initial-deal failures, without invented identities.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ModelError {
    /// Full cascade cannot accept another card.
    #[error("FreeCell cascade exceeds 20 cards")]
    Overflow,
    /// Empty cascade cannot be popped.
    #[error("FreeCell cascade is empty")]
    Underflow,
    /// Location does not refer to supported occupied storage.
    #[error("FreeCell location is out of bounds")]
    Location,
    /// Progress cannot exceed King.
    #[error("FreeCell foundation {0} exceeds King")]
    Foundation(Suit),
    /// Initial physical column has an incorrect length.
    #[error("FreeCell PLAY {column} has {actual} cards; expected {expected}")]
    InitialLength {
        /// One-based display column.
        column: usize,
        /// Required initial count.
        expected: usize,
        /// Actual supplied count.
        actual: usize,
    },
    /// T=0 requires empty top slots.
    #[error("FreeCell initial top area is occupied at {0:?}")]
    InitialTop(Location),
    /// Exact duplicate evidence retains both original physical locations.
    #[error("FreeCell duplicate {card} at {first:?} and {second:?}")]
    Duplicate {
        /// Repeated shared identity.
        card: Card,
        /// First physical location.
        first: Location,
        /// Second physical location.
        second: Location,
    },
    /// No substitution or elimination can complete a missing deck.
    #[error("FreeCell initial inventory is not the exact 52-card deck")]
    IncompleteDeck,
    /// Descriptive metadata is empty, oversized or contains path/control characters.
    #[error("FreeCell descriptive metadata is invalid")]
    Metadata,
}

/// Fixed-capacity occupied prefix and empty tail, with no fabricated sentinel card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CascadeStack {
    cards: [Option<Card>; CASCADE_CAPACITY],
    len: usize,
}

impl Default for CascadeStack {
    fn default() -> Self {
        Self {
            cards: [None; CASCADE_CAPACITY],
            len: 0,
        }
    }
}

impl CascadeStack {
    /// Collect cards in supplied top-to-bottom order.
    ///
    /// # Errors
    /// Returns `Overflow` on a twenty-first identity.
    pub fn from_cards(cards: impl IntoIterator<Item = Card>) -> Result<Self, ModelError> {
        let mut result = Self::default();
        for card in cards {
            result.push(card)?;
        }
        Ok(result)
    }
    /// Occupied prefix length.
    pub const fn len(&self) -> usize {
        self.len
    }
    /// Whether no identity is stored.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Borrow occupied identities in physical order.
    pub fn iter(&self) -> impl Iterator<Item = Card> + '_ {
        self.cards.iter().take(self.len).flatten().copied()
    }
    /// Read one occupied depth.
    ///
    /// # Errors
    /// Returns `Location` for an unoccupied or unsupported depth.
    pub fn get(&self, depth: usize) -> Result<Card, ModelError> {
        if depth >= self.len {
            return Err(ModelError::Location);
        }
        self.cards
            .get(depth)
            .copied()
            .flatten()
            .ok_or(ModelError::Location)
    }
    /// Append one exposed identity.
    ///
    /// # Errors
    /// Returns `Overflow` without mutation when full.
    pub fn push(&mut self, card: Card) -> Result<(), ModelError> {
        *self.cards.get_mut(self.len).ok_or(ModelError::Overflow)? = Some(card);
        self.len += 1;
        Ok(())
    }
    /// Remove the exposed identity and erase its former slot.
    ///
    /// # Errors
    /// Returns `Underflow` without mutation when empty.
    pub fn pop(&mut self) -> Result<Card, ModelError> {
        let index = self.len.checked_sub(1).ok_or(ModelError::Underflow)?;
        let card = self
            .cards
            .get_mut(index)
            .ok_or(ModelError::Location)?
            .take()
            .ok_or(ModelError::Location)?;
        self.len = index;
        Ok(card)
    }
}

/// Full board shape, separate from initial validation and future move legality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardState {
    /// Physical PLAY columns.
    cascades: [CascadeStack; 8],
    /// Physical CELL slots.
    cells: [Option<Card>; 4],
    /// Canonical C,D,H,S progress.
    foundations: [u8; 4],
}

impl BoardState {
    /// Construct storage with checked foundation progress.
    ///
    /// # Errors
    /// Returns `Foundation` for progress beyond thirteen; does not claim T=0.
    pub fn new(
        cascades: [CascadeStack; 8],
        cells: [Option<Card>; 4],
        foundations: [u8; 4],
    ) -> Result<Self, ModelError> {
        for (count, suit) in foundations.iter().zip(FOUNDATION_SUITS) {
            if *count > 13 {
                return Err(ModelError::Foundation(suit));
            }
        }
        Ok(Self {
            cascades,
            cells,
            foundations,
        })
    }
    /// Original physical columns.
    pub const fn cascades(&self) -> &[CascadeStack; 8] {
        &self.cascades
    }
    /// Physical temporary cells.
    pub const fn cells(&self) -> &[Option<Card>; 4] {
        &self.cells
    }
    /// Canonical foundation progress.
    pub const fn foundations(&self) -> &[u8; 4] {
        &self.foundations
    }
}

/// Validated immutable complete original deal, never a live execution permit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialDeal {
    board: BoardState,
    metadata: Metadata,
}

/// Bounded descriptive labels, never host paths or execution authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    deal_id: String,
    profile: String,
    source: String,
}

impl Metadata {
    /// Check three nonempty labels of at most 128 UTF-8 bytes each.
    ///
    /// # Errors
    /// Refuses path separators, control characters and oversized labels.
    pub fn new(
        deal_id: impl Into<String>,
        profile: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let labels = [deal_id.into(), profile.into(), source.into()];
        if labels.iter().any(|s| {
            s.is_empty()
                || s.len() > 128
                || s.chars().any(|c| c.is_control() || c == '/' || c == '\\')
        }) {
            return Err(ModelError::Metadata);
        }
        let [deal_id, profile, source] = labels;
        Ok(Self {
            deal_id,
            profile,
            source,
        })
    }
    /// Stable deal label.
    pub fn deal_id(&self) -> &str {
        &self.deal_id
    }
    /// Measured recognition profile label.
    pub fn profile(&self) -> &str {
        &self.profile
    }
    /// Descriptive acquisition source label.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl InitialDeal {
    /// Require exact initial shape, empty top areas and all 52 unique identities.
    ///
    /// # Errors
    /// Returns the first shape, top-area or duplicate failure. Never infers cards.
    pub fn new(board: BoardState, metadata: Metadata) -> Result<Self, ModelError> {
        for (column, (stack, expected)) in board.cascades.iter().zip(INITIAL_LENGTHS).enumerate() {
            if stack.len() != expected {
                return Err(ModelError::InitialLength {
                    column: column + 1,
                    expected,
                    actual: stack.len(),
                });
            }
        }
        for (index, cell) in board.cells.iter().enumerate() {
            if cell.is_some() {
                return Err(ModelError::InitialTop(Location::Cell(index)));
            }
        }
        for (count, suit) in board.foundations.iter().zip(FOUNDATION_SUITS) {
            if *count != 0 {
                return Err(ModelError::InitialTop(Location::Foundation(suit)));
            }
        }
        let mut seen = [None; 52];
        let mut mask = 0_u64;
        for (column, stack) in board.cascades.iter().enumerate() {
            for (depth, card) in stack.iter().enumerate() {
                let location = Location::Cascade(CascadeLocation::new(column, depth)?);
                let suit = match card.suit() {
                    Suit::Clubs => 0_usize,
                    Suit::Diamonds => 1,
                    Suit::Hearts => 2,
                    Suit::Spades => 3,
                };
                let rank = usize::from(card.rank().value())
                    .checked_sub(1)
                    .ok_or(ModelError::IncompleteDeck)?;
                let id = suit
                    .checked_mul(13)
                    .and_then(|base| base.checked_add(rank))
                    .ok_or(ModelError::IncompleteDeck)?;
                let previous = seen.get_mut(id).ok_or(ModelError::IncompleteDeck)?;
                if let Some(first) = *previous {
                    return Err(ModelError::Duplicate {
                        card,
                        first,
                        second: location,
                    });
                }
                *previous = Some(location);
                mask |= 1_u64
                    .checked_shl(u32::try_from(id).map_err(|_| ModelError::IncompleteDeck)?)
                    .ok_or(ModelError::IncompleteDeck)?;
            }
        }
        if mask != (1_u64 << 52) - 1 {
            return Err(ModelError::IncompleteDeck);
        }
        Ok(Self { board, metadata })
    }
    /// Borrow original identities without canonical sorting.
    pub const fn board(&self) -> &BoardState {
        &self.board
    }
    /// Borrow bounded descriptive metadata.
    pub const fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}
