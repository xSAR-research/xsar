//! Pure single-card Free Cell rules with immutable physical column/cell indices.
//!
//! Cards are compact identity IDs (C,D,H,S blocks of thirteen), not copied deal
//! records. Equality permits column/cell permutations but compares complete ordered
//! contents; cached column hashes are never evidence of equality. No guest policy,
//! transport, input, timing or configured resource limits live here.

use super::InitialDeal;
use crate::games::cards::Card;
use std::hash::{Hash, Hasher};


/// Empty storage, outside the closed 0..52 card-ID domain.
const EMPTY: u8 = u8::MAX;


/// A physical source/destination; foundations are fixed canonical C,D,H,S.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pile {
    /// Zero-based PLAY column, from left to right.
    Cascade(u8),
    /// Zero-based temporary CELL, from left to right.
    Cell(u8),
    /// Zero-based canonical suit, never a legal source.
    Foundation(u8),
}


/// One physical card transfer; automatic transfers have zero declared cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordedMove {
    /// Closed C,D,H,S identity ID, suit*13 + rank-1.
    pub card_id: u8,
    /// Actual physical source in the parent position.
    pub from: Pile,
    /// Actual physical destination in the child position.
    pub to: Pile,
    /// True only for the deterministic conservative automatic transfer.
    pub is_auto: bool,
}


/// Checked physical storage/rule failure; no mutation is committed on error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RuleError {
    /// Invalid pile, absent/exposed identity mismatch, destination or auto policy.
    #[error("Illegal Free Cell card transfer")]
    IllegalMove,
    /// Storage, cached identity or complete-inventory invariant is inconsistent.
    #[error("Invalid Free Cell physical position")]
    InvalidPosition,
}


/// Compact physical position: columns occupy consecutive ordered slices of cards.
///
/// Unoccupied tail bytes are EMPTY. Only touched columns have their cached hashes
/// recomputed after a move; canonical comparisons never rewrite physical storage.
#[derive(Clone, Copy, Debug)]
pub struct Position {
    /// Live PLAY identities, concatenated in physical column order.
    cards: [u8; 52],
    /// Physical column lengths, each at most the model capacity twenty.
    lengths: [u8; 8],
    /// Physical cell identities or EMPTY.
    cells: [u8; 4],
    /// Irreversible canonical C,D,H,S progress, 0..=13.
    foundations: [u8; 4],
    /// Incremental per-column accelerators, never equality keys alone.
    column_hashes: [u64; 8],
}


impl Position {
    /// Encode the validated immutable original deal without sorting its columns.
    pub fn initial(deal: &InitialDeal) -> Self {
        let mut result = Self {
            cards: [EMPTY; 52],
            lengths: [0; 8],
            cells: [EMPTY; 4],
            foundations: [0; 4],
            column_hashes: [0; 8],
        };
        let mut offset = 0;
        for (column, stack) in deal.board().cascades().iter().enumerate() {
            if let Some(length) = result.lengths.get_mut(column) {
                *length = u8::try_from(stack.len()).unwrap_or(0);
            }
            for card in stack.iter() {
                if let Some(slot) = result.cards.get_mut(offset) {
                    *slot = card_id(card);
                }
                offset += 1;
            }
        }
        for column in 0..8 {
            result.refresh(column);
        }
        result
    }


    /// Borrow one complete physical column, or None for a malformed index.
    pub fn column(&self, column: usize) -> Option<&[u8]> {
        let length = usize::from(*self.lengths.get(column)?);
        let start: usize = self
            .lengths
            .get(..column)?
            .iter()
            .map(|n| usize::from(*n))
            .sum();
        self.cards.get(start..start.checked_add(length)?)
    }


    /// Borrow physical temporary cells; None means positively empty model storage.
    pub fn cells(&self) -> [Option<u8>; 4] {
        self.cells.map(|id| (id != EMPTY).then_some(id))
    }


    /// Borrow canonical suit progress, independent of screen decoration order.
    pub const fn foundations(&self) -> &[u8; 4] {
        &self.foundations
    }


    /// Count all cards already placed into irreversible suit foundations.
    pub fn foundation_cards(&self) -> u32 {
        self.foundations.iter().map(|n| u32::from(*n)).sum()
    }


    /// True only when all52 cards are banked and every PLAY/CELL is empty.
    pub fn is_goal(&self) -> bool {
        self.foundations == [13; 4] && self.lengths == [0; 8] && self.cells == [EMPTY; 4]
    }


    /// Validate all52 identities and the checked packed-storage/cache invariants.
    ///
    /// # Errors
    /// Rejects missing/duplicate identities, malformed storage or stale hashes.
    pub fn validate(&self) -> Result<(), RuleError> {
        let mut seen = 0_u64;
        let mut add = |id: u8| -> Result<(), RuleError> {
            if id >= 52 {
                return Err(RuleError::InvalidPosition);
            }
            let bit = 1_u64 << id;
            if seen & bit != 0 {
                return Err(RuleError::InvalidPosition);
            }
            seen |= bit;
            Ok(())
        };
        for (column, (&length, &cached)) in self.lengths.iter().zip(&self.column_hashes).enumerate()
        {
            if usize::from(length) > super::CASCADE_CAPACITY {
                return Err(RuleError::InvalidPosition);
            }
            let cards = self.column(column).ok_or(RuleError::InvalidPosition)?;
            if hash_column(cards) != cached {
                return Err(RuleError::InvalidPosition);
            }
            for &id in cards {
                add(id)?;
            }
        }
        let live: usize = self.lengths.iter().map(|n| usize::from(*n)).sum();
        if !self
            .cards
            .get(live..)
            .is_some_and(|tail| tail.iter().all(|id| *id == EMPTY))
        {
            return Err(RuleError::InvalidPosition);
        }
        for &id in &self.cells {
            if id != EMPTY {
                add(id)?;
            }
        }
        for (suit, &rank) in self.foundations.iter().enumerate() {
            if rank > 13 {
                return Err(RuleError::InvalidPosition);
            }
            for value in 0..rank {
                add(u8::try_from(suit * 13).map_err(|_| RuleError::InvalidPosition)? + value)?;
            }
        }
        if seen != (1_u64 << 52) - 1 {
            return Err(RuleError::InvalidPosition);
        }
        Ok(())
    }


    /// Return the deterministic first safe automatic transfer, if one exists.
    pub fn automatic(&self) -> Option<RecordedMove> {
        for from in (0..8).map(Pile::Cascade).chain((0..4).map(Pile::Cell)) {
            let Some(id) = self.exposed(from) else {
                continue;
            };
            if self.foundation_accepts(id) && self.safe_auto(id) {
                return Some(RecordedMove {
                    card_id: id,
                    from,
                    to: Pile::Foundation(id / 13),
                    is_auto: true,
                });
            }
        }
        None
    }


    /// Generate all rule/symmetry successors without per-node heap allocation.
    /// Safe automatic moves are explicit forced edges, including the root closure.
    pub fn successors(self) -> Successors {
        Successors {
            state: self,
            source: 0,
            destination: 0,
            automatic_done: false,
        }
    }


    /// Apply one checked legal physical move to a copy.
    ///
    /// # Errors
    /// Rejects illegal exposure/destination/auto policy or malformed indices.
    pub fn apply(self, action: RecordedMove) -> Result<Self, RuleError> {
        if self.exposed(action.from) != Some(action.card_id)
            || action.card_id >= 52
            || action.from == action.to
        {
            return Err(RuleError::IllegalMove);
        }
        if action.is_auto && self.automatic() != Some(action) {
            return Err(RuleError::IllegalMove);
        }
        match action.to {
            Pile::Foundation(suit)
                if suit == action.card_id / 13 && self.foundation_accepts(action.card_id) => {}
            Pile::Cascade(column) => {
                let stack = self
                    .column(usize::from(column))
                    .ok_or(RuleError::IllegalMove)?;
                if stack.len() >= super::CASCADE_CAPACITY
                    || stack.last().is_some_and(|&top| {
                        rank(top) != rank(action.card_id) + 1 || red(top) == red(action.card_id)
                    })
                {
                    return Err(RuleError::IllegalMove);
                }
            }
            Pile::Cell(cell)
                if matches!(action.from, Pile::Cascade(_))
                    && self.cells.get(usize::from(cell)) == Some(&EMPTY) => {}
            _ => return Err(RuleError::IllegalMove),
        }
        let mut next = self;
        next.remove(action.from)?;
        next.place(action.to, action.card_id)?;
        if let Pile::Cascade(c) = action.from {
            next.refresh(usize::from(c));
        }
        if let Pile::Cascade(c) = action.to {
            next.refresh(usize::from(c));
        }
        Ok(next)
    }


    /// Ordering guidance only: remaining cards plus next-needed burial depths.
    /// It is not an admissible lower bound with zero-cost automatic transfers.
    pub fn guidance(self) -> u32 {
        let mut value = 52 - self.foundation_cards();
        for column in 0..8 {
            if let Some(cards) = self.column(column) {
                for (depth, &id) in cards.iter().enumerate() {
                    if self.foundation_accepts(id) {
                        value += u32::try_from(cards.len() - depth - 1).unwrap_or(0);
                    }
                }
            }
        }
        value
    }


    /// Read only an exposed cascade/cell card; foundations are never sources.
    fn exposed(&self, from: Pile) -> Option<u8> {
        match from {
            Pile::Cascade(column) => self.column(usize::from(column))?.last().copied(),
            Pile::Cell(cell) => self
                .cells
                .get(usize::from(cell))
                .copied()
                .filter(|id| *id != EMPTY),
            Pile::Foundation(_) => None,
        }
    }


    /// Check exact next-rank suit progression.
    fn foundation_accepts(&self, id: u8) -> bool {
        self.foundations
            .get(usize::from(id / 13))
            .is_some_and(|n| *n + 1 == rank(id))
    }


    /// Conservative approved root/interleaved auto-transfer rule.
    fn safe_auto(&self, id: u8) -> bool {
        rank(id) <= 2
            || self
                .foundations
                .iter()
                .enumerate()
                .filter(|(suit, _)| (*suit == 1 || *suit == 2) != red(id))
                .all(|(_, n)| *n >= rank(id) - 1)
    }


    /// Remove exactly the exposed physical identity from packed storage.
    fn remove(&mut self, from: Pile) -> Result<(), RuleError> {
        match from {
            Pile::Cell(c) => {
                *self
                    .cells
                    .get_mut(usize::from(c))
                    .ok_or(RuleError::IllegalMove)? = EMPTY
            }
            Pile::Cascade(c) => {
                let column = usize::from(c);
                let start: usize = self
                    .lengths
                    .get(..column)
                    .ok_or(RuleError::IllegalMove)?
                    .iter()
                    .map(|n| usize::from(*n))
                    .sum();
                let length = self.lengths.get_mut(column).ok_or(RuleError::IllegalMove)?;
                let end = start
                    .checked_add(usize::from(*length))
                    .and_then(|n| n.checked_sub(1))
                    .ok_or(RuleError::IllegalMove)?;
                self.cards
                    .get_mut(end..)
                    .ok_or(RuleError::InvalidPosition)?
                    .rotate_left(1);
                *self.cards.last_mut().ok_or(RuleError::InvalidPosition)? = EMPTY;
                *length = length.checked_sub(1).ok_or(RuleError::IllegalMove)?;
            }
            _ => return Err(RuleError::IllegalMove),
        }
        Ok(())
    }


    /// Place a previously removed card without changing any unrelated pile.
    fn place(&mut self, to: Pile, id: u8) -> Result<(), RuleError> {
        match to {
            Pile::Cell(c) => {
                *self
                    .cells
                    .get_mut(usize::from(c))
                    .ok_or(RuleError::IllegalMove)? = id
            }
            Pile::Foundation(s) => {
                let count = self
                    .foundations
                    .get_mut(usize::from(s))
                    .ok_or(RuleError::IllegalMove)?;
                *count = count.checked_add(1).ok_or(RuleError::InvalidPosition)?;
            }
            Pile::Cascade(c) => {
                let column = usize::from(c);
                let end: usize = self
                    .lengths
                    .get(..=column)
                    .ok_or(RuleError::IllegalMove)?
                    .iter()
                    .map(|n| usize::from(*n))
                    .sum();
                self.cards
                    .get_mut(end..)
                    .ok_or(RuleError::InvalidPosition)?
                    .rotate_right(1);
                *self.cards.get_mut(end).ok_or(RuleError::InvalidPosition)? = id;
                let length = self.lengths.get_mut(column).ok_or(RuleError::IllegalMove)?;
                *length = length.checked_add(1).ok_or(RuleError::InvalidPosition)?;
            }
        }
        Ok(())
    }


    /// Refresh only one changed column accelerator.
    fn refresh(&mut self, column: usize) {
        let hash = self.column(column).map(hash_column);
        if let (Some(slot), Some(hash)) = (self.column_hashes.get_mut(column), hash) {
            *slot = hash;
        }
    }


    /// Exact ordered column sequences sorted for symmetry comparison only.
    fn canonical_columns(&self) -> [&[u8]; 8] {
        let mut columns = std::array::from_fn(|c| self.column(c).unwrap_or(&[]));
        columns.sort_unstable();
        columns
    }
}


impl PartialEq for Position {
    /// Full exact equality modulo permitted interchangeable columns and cells.
    fn eq(&self, other: &Self) -> bool {
        if self.foundations != other.foundations {
            return false;
        }
        let mut left = self.cells;
        let mut right = other.cells;
        left.sort_unstable();
        right.sort_unstable();
        left == right && self.canonical_columns() == other.canonical_columns()
    }
}


impl Eq for Position {}


impl Hash for Position {
    /// Incremental hashes accelerate lookup; Eq still verifies complete sequences.
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut columns = self.column_hashes;
        let mut cells = self.cells;
        columns.sort_unstable();
        cells.sort_unstable();
        columns.hash(state);
        cells.hash(state);
        self.foundations.hash(state);
    }
}


/// Finite, deterministic, allocation-free iterator over explicit physical moves.
pub struct Successors {
    /// Parent physical representative, never a dictionary canonical key.
    state: Position,
    /// Next physical cascade/cell source, 0..12.
    source: u8,
    /// Next foundation/cascade/cell destination, 0..13.
    destination: u8,
    /// Whether a forced automatic edge was already emitted.
    automatic_done: bool,
}


impl Iterator for Successors {
    type Item = (RecordedMove, Position);


    /// Prefer explicit foundation, then physical PLAY and CELL destinations.
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(action) = self.state.automatic() {
            if self.automatic_done {
                return None;
            }
            self.automatic_done = true;
            return self.state.apply(action).ok().map(|next| (action, next));
        }
        while self.source < 12 {
            let from = if self.source < 8 {
                Pile::Cascade(self.source)
            } else {
                Pile::Cell(self.source - 8)
            };
            let Some(id) = self.state.exposed(from) else {
                self.source += 1;
                self.destination = 0;
                continue;
            };
            while self.destination < 13 {
                let dest = self.destination;
                self.destination += 1;
                let to = match dest {
                    0 => Pile::Foundation(id / 13),
                    1..=8 => Pile::Cascade(dest - 1),
                    _ => Pile::Cell(dest - 9),
                };
                match to {
                    Pile::Cascade(c)
                        if self
                            .state
                            .column(usize::from(c))
                            .is_some_and(|s| s.is_empty()) =>
                    {
                        if self.state.lengths.iter().position(|n| *n == 0) != Some(usize::from(c)) {
                            continue;
                        }
                        if matches!(from, Pile::Cascade(source) if self.state.column(usize::from(source)).is_some_and(|s| s.len() == 1))
                        {
                            continue;
                        }
                    }
                    Pile::Cell(c)
                        if self.state.cells.iter().position(|id| *id == EMPTY)
                            != Some(usize::from(c)) =>
                    {
                        continue;
                    }
                    _ => {}
                }
                let action = RecordedMove {
                    card_id: id,
                    from,
                    to,
                    is_auto: false,
                };
                if let Ok(next) = self.state.apply(action) {
                    return Some((action, next));
                }
            }
            self.source += 1;
            self.destination = 0;
        }
        None
    }
}


/// Closed identity encoding, independent of the original card's physical position.
pub fn card_id(card: Card) -> u8 {
    let suit = match card.suit() {
        super::super::cards::Suit::Clubs => 0,
        super::super::cards::Suit::Diamonds => 1,
        super::super::cards::Suit::Hearts => 2,
        super::super::cards::Suit::Spades => 3,
    };
    suit * 13 + card.rank().value() - 1
}


/// One-based closed rank for a validated identity.
fn rank(id: u8) -> u8 {
    id % 13 + 1
}


/// Shared red/black colour in canonical C,D,H,S order.
fn red(id: u8) -> bool {
    matches!(id / 13, 1 | 2)
}


/// Stable ordered hash of one complete sequence, including its length.
fn hash_column(cards: &[u8]) -> u64 {
    cards
        .iter()
        .fold(0xcbf29ce484222325_u64 ^ cards.len() as u64, |h, id| {
            (h ^ u64::from(*id)).wrapping_mul(0x100000001b3)
        })
}


#[cfg(test)]
mod tests;
