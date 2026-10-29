//! Cards use rank-major encoding: 0 = 2c, 1 = 2d, ..., 51 = As.
use std::{error::Error, fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardError {
    InvalidCard,
    DuplicateCard,
    InvalidCardCount,
}
impl fmt::Display for CardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCard => "expected card ID 0..51 or notation such as As or Td",
            Self::DuplicateCard => "a card appears more than once",
            Self::InvalidCardCount => "invalid number of cards",
        })
    }
}
impl Error for CardError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Card(u8);
impl Card {
    pub fn from_id(id: u8) -> Result<Self, CardError> {
        if id < 52 {
            Ok(Self(id))
        } else {
            Err(CardError::InvalidCard)
        }
    }
    pub const fn id(self) -> u8 {
        self.0
    }
    /// Rank in 0..13: deuce = 0, ace = 12.
    pub const fn rank(self) -> u8 {
        self.0 / 4
    }
    /// Clubs, diamonds, hearts, spades = 0, 1, 2, 3.
    pub const fn suit(self) -> u8 {
        self.0 % 4
    }
    pub const fn mask(self) -> u64 {
        1u64 << self.0
    }
}
impl FromStr for Card {
    type Err = CardError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != 2 {
            return Err(CardError::InvalidCard);
        }
        let rank = b"23456789TJQKA"
            .iter()
            .position(|&x| x == b[0].to_ascii_uppercase());
        let suit = b"cdhs".iter().position(|&x| x == b[1].to_ascii_lowercase());
        match (rank, suit) {
            (Some(r), Some(s)) => Ok(Self((r * 4 + s) as u8)),
            _ => Err(CardError::InvalidCard),
        }
    }
}
impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            b"23456789TJQKA"[self.rank() as usize] as char,
            b"cdhs"[self.suit() as usize] as char
        )
    }
}

/// Validate uniqueness and return the 52-bit card set.
pub fn card_mask(cards: &[Card]) -> Result<u64, CardError> {
    let mut mask = 0;
    for &c in cards {
        if mask & c.mask() != 0 {
            return Err(CardError::DuplicateCard);
        }
        mask |= c.mask();
    }
    Ok(mask)
}

/// Unordered private hand in canonical card-ID order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HoleCards([Card; 2]);
impl HoleCards {
    pub fn new(a: Card, b: Card) -> Result<Self, CardError> {
        if a == b {
            return Err(CardError::DuplicateCard);
        }
        Ok(Self(if a < b { [a, b] } else { [b, a] }))
    }
    pub const fn cards(self) -> [Card; 2] {
        self.0
    }
    pub fn mask(self) -> u64 {
        self.0[0].mask() | self.0[1].mask()
    }
}

/// A public Hold'em board; only street boundaries (0, 3, 4, 5) are accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board(Vec<Card>);
impl Board {
    pub fn new(cards: &[Card]) -> Result<Self, CardError> {
        if !matches!(cards.len(), 0 | 3 | 4 | 5) {
            return Err(CardError::InvalidCardCount);
        }
        card_mask(cards)?;
        Ok(Self(cards.to_vec()))
    }
    pub fn cards(&self) -> &[Card] {
        &self.0
    }
    pub fn mask(&self) -> u64 {
        card_mask(&self.0).expect("board invariant")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoding_round_trip() {
        assert_eq!(std::mem::size_of::<Card>(), 1);
        for id in 0..52 {
            let c = Card::from_id(id).unwrap();
            assert_eq!(c.to_string().parse::<Card>().unwrap(), c);
            assert_eq!(c.mask().count_ones(), 1);
        }
        for s in ["", "10s", "Ax", "1c", "🂡"] {
            assert!(s.parse::<Card>().is_err());
        }
        assert!(Card::from_id(52).is_err());
    }
    #[test]
    fn sets_hands_boards() {
        let a: Card = "As".parse().unwrap();
        let b: Card = "Kd".parse().unwrap();
        assert_eq!(HoleCards::new(a, b), HoleCards::new(b, a));
        assert!(HoleCards::new(a, a).is_err());
        assert!(card_mask(&[a, a]).is_err());
        assert!(Board::new(&[a, b]).is_err());
        assert!(Board::new(&[a, b, a]).is_err());
        assert_eq!(Board::new(&[]).unwrap().mask(), 0);
        let c: Card = "2c".parse().unwrap();
        let board = Board::new(&[a, b, c]).unwrap();
        assert_eq!(board.cards().len(), 3);
        assert_eq!(board.mask().count_ones(), 3);
        assert_eq!(HoleCards::new(a, b).unwrap().mask().count_ones(), 2);
    }
}
