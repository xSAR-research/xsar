//! Pure standard-deck card identities without recognition, coordinates or rules.
//!
//! Unknown observations belong to the acquiring application, not to a card value.
//! Enumeration order is Ace through King, and clubs, diamonds, hearts, spades.

use std::fmt;


/// One of the thirteen standard ranks, with no joker or unknown value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Rank {
    /// Ace.
    Ace,
    /// Two.
    Two,
    /// Three.
    Three,
    /// Four.
    Four,
    /// Five.
    Five,
    /// Six.
    Six,
    /// Seven.
    Seven,
    /// Eight.
    Eight,
    /// Nine.
    Nine,
    /// Ten.
    Ten,
    /// Jack.
    Jack,
    /// Queen.
    Queen,
    /// King.
    King,
}


impl Rank {
    /// Every rank, in Ace-through-King order.
    pub const ALL: [Self; 13] = [
        Self::Ace,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
    ];


    /// Return the fixed Ace-low numeric rank, from Ace = 1 through King = 13.
    ///
    /// This is not a game-specific score or proof that a card can be played.
    /// Suit, exposed-card rules and any Ace/King adjacency policy stay separate.
    pub const fn value(self) -> u8 {
        match self {
            Self::Ace => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
            Self::Nine => 9,
            Self::Ten => 10,
            Self::Jack => 11,
            Self::Queen => 12,
            Self::King => 13,
        }
    }


    /// Interpret a fixed Ace-low rank value; zero and values above 13 are invalid.
    pub const fn from_value(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Ace),
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            4 => Some(Self::Four),
            5 => Some(Self::Five),
            6 => Some(Self::Six),
            7 => Some(Self::Seven),
            8 => Some(Self::Eight),
            9 => Some(Self::Nine),
            10 => Some(Self::Ten),
            11 => Some(Self::Jack),
            12 => Some(Self::Queen),
            13 => Some(Self::King),
            _ => None,
        }
    }


    /// Canonical portable rank code, independent of font or artwork.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ace => "A",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
        }
    }
}


impl fmt::Display for Rank {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}


/// One of the four standard suits, independent of colour or screen artwork.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Suit {
    /// Clubs.
    Clubs,
    /// Diamonds.
    Diamonds,
    /// Hearts.
    Hearts,
    /// Spades.
    Spades,
}


impl Suit {
    /// Every suit, in clubs, diamonds, hearts, spades order.
    pub const ALL: [Self; 4] = [Self::Clubs, Self::Diamonds, Self::Hearts, Self::Spades];


    /// Canonical lower-case portable suit name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clubs => "clubs",
            Self::Diamonds => "diamonds",
            Self::Hearts => "hearts",
            Self::Spades => "spades",
        }
    }
}


impl fmt::Display for Suit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}


/// A standard rank-and-suit identity, not a sighting or physical position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Card {
    /// Validated rank identity.
    rank: Rank,
    /// Validated suit identity.
    suit: Suit,
}


impl Card {
    /// Construct an identity from the closed rank and suit domains.
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }


    /// Return the rank without altering the identity.
    pub const fn rank(self) -> Rank {
        self.rank
    }


    /// Return the suit without altering the identity.
    pub const fn suit(self) -> Suit {
        self.suit
    }
}


impl fmt::Display for Card {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} of {}", self.rank, self.suit)
    }
}


#[cfg(test)]
mod tests {
    //! Identity coverage independent of Pyramid and serialisation features.

    use super::*;
    use std::collections::HashSet;


    /// Numeric ranks are const, exact, suit-independent and closed over all u8 inputs.
    #[test]
    fn numeric_rank_values_are_exact_and_checked() {
        const ACE: Option<Rank> = Rank::from_value(1);
        const KING: u8 = Rank::King.value();
        assert_eq!(ACE, Some(Rank::Ace));
        assert_eq!(KING, 13);
        assert_eq!(Rank::ALL.map(Rank::value), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);


        for value in 0..=u8::MAX {
            let expected = value.checked_sub(1).and_then(|index| Rank::ALL.get(usize::from(index))).copied();
            assert_eq!(Rank::from_value(value), expected);
        }
    }


    /// Enumerating ranks and suits yields exactly the standard 52 identities.
    #[test]
    fn identities_and_canonical_names_are_unique() {
        let cards: HashSet<_> = Suit::ALL
            .into_iter()
            .flat_map(|suit| Rank::ALL.into_iter().map(move |rank| Card::new(rank, suit)))
            .collect();
        assert_eq!(cards.len(), 52);
        assert_eq!(
            Rank::ALL.map(Rank::as_str),
            ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K",]
        );
        assert_eq!(
            Suit::ALL.map(Suit::as_str),
            ["clubs", "diamonds", "hearts", "spades"]
        );
        let card = Card::new(Rank::Queen, Suit::Hearts);
        assert_eq!(card.rank(), Rank::Queen);
        assert_eq!(card.suit(), Suit::Hearts);
        assert_eq!(card.to_string(), "Q of hearts");
    }
}
