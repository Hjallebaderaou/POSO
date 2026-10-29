//! Direct rank-count / suit-bitmask evaluator. No tables, allocation, or subset
//! enumeration in the production path. Larger HandValues always win.
use crate::cards::{card_mask, Card, CardError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Category {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

/// Category followed by five rank nibbles, most significant first.
/// Unused nibbles are zero; suits never break ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HandValue(u32);
impl HandValue {
    pub fn category(self) -> Category {
        match self.0 >> 20 {
            0 => Category::HighCard,
            1 => Category::OnePair,
            2 => Category::TwoPair,
            3 => Category::ThreeOfAKind,
            4 => Category::Straight,
            5 => Category::Flush,
            6 => Category::FullHouse,
            7 => Category::FourOfAKind,
            8 => Category::StraightFlush,
            _ => unreachable!(),
        }
    }
    /// Descending significance, ranks 0=deuce through 12=ace.
    pub fn tiebreakers(self) -> [u8; 5] {
        std::array::from_fn(|i| ((self.0 >> (16 - 4 * i)) & 15) as u8)
    }
}
fn value(category: Category, ranks: &[u8]) -> HandValue {
    let mut v = (category as u32) << 20;
    for (i, &rank) in ranks.iter().enumerate() {
        v |= (rank as u32) << (16 - i * 4);
    }
    HandValue(v)
}
fn straight(mask: u16) -> Option<u8> {
    for high in (4..=12).rev() {
        if (mask >> (high - 4)) & 31 == 31 {
            return Some(high);
        }
    }
    if mask & 0x100f == 0x100f {
        Some(3)
    } else {
        None
    }
}
fn top(mask: u16, n: usize) -> [u8; 5] {
    let mut out = [0; 5];
    let mut i = 0;
    for rank in (0..13).rev() {
        if mask & (1 << rank) != 0 {
            out[i] = rank;
            i += 1;
            if i == n {
                break;
            }
        }
    }
    out
}
/// Exact best-five ranking of 5, 6, or 7 distinct cards.
/// For Hold'em pass both hole cards and the board; there is no requirement to
/// use either hole card. Duplicate cards are rejected across the entire input.
pub fn evaluate(cards: &[Card]) -> Result<HandValue, CardError> {
    if !(5..=7).contains(&cards.len()) {
        return Err(CardError::InvalidCardCount);
    }
    card_mask(cards)?;
    let mut counts = [0u8; 13];
    let mut suits = [0u16; 4];
    let mut mask = 0u16;
    for c in cards {
        counts[c.rank() as usize] += 1;
        suits[c.suit() as usize] |= 1 << c.rank();
        mask |= 1 << c.rank();
    }
    let flush = suits.iter().copied().find(|m| m.count_ones() >= 5);
    if let Some(m) = flush {
        if let Some(high) = straight(m) {
            return Ok(value(Category::StraightFlush, &[high]));
        }
    }
    let highest = |n| (0..13).rev().find(|&r| counts[r] >= n).map(|r| r as u8);
    if let Some(q) = highest(4) {
        return Ok(value(
            Category::FourOfAKind,
            &[q, top(mask & !(1 << q), 1)[0]],
        ));
    }
    let trips = highest(3);
    if let Some(t) = trips {
        if let Some(p) = (0..13).rev().find(|&r| r != t as usize && counts[r] >= 2) {
            return Ok(value(Category::FullHouse, &[t, p as u8]));
        }
    }
    if let Some(m) = flush {
        return Ok(value(Category::Flush, &top(m, 5)));
    }
    if let Some(high) = straight(mask) {
        return Ok(value(Category::Straight, &[high]));
    }
    if let Some(t) = trips {
        let k = top(mask & !(1 << t), 2);
        return Ok(value(Category::ThreeOfAKind, &[t, k[0], k[1]]));
    }
    if let Some(p) = highest(2) {
        if let Some(q) = (0..p).rev().find(|&r| counts[r as usize] >= 2) {
            let k = top(mask & !(1 << p) & !(1 << q), 1);
            return Ok(value(Category::TwoPair, &[p, q, k[0]]));
        }
        let k = top(mask & !(1 << p), 3);
        return Ok(value(Category::OnePair, &[p, k[0], k[1], k[2]]));
    }
    Ok(value(Category::HighCard, &top(mask, 5)))
}
