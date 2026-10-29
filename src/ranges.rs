//! Dense 1,326-combo ranges and blocker-correct joint distributions.
//! Weights are finite, nonnegative relative masses, not automatically normalized.
use crate::cards::{card_mask, Board, Card, CardError, HoleCards};
use std::{error::Error, fmt, sync::OnceLock};

pub const COMBO_COUNT: usize = 1326;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    InvalidIndex,
    InvalidWeight,
    EmptyRange,
    NoCompatibleHands,
    NumericalUnderflow,
    Cards(CardError),
}
impl From<CardError> for RangeError {
    fn from(e: CardError) -> Self {
        Self::Cards(e)
    }
}
impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for RangeError {}

/// Stable lexicographic index over (low_card_id, high_card_id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComboIndex(usize);
impl ComboIndex {
    pub fn new(index: usize) -> Result<Self, RangeError> {
        if index < COMBO_COUNT {
            Ok(Self(index))
        } else {
            Err(RangeError::InvalidIndex)
        }
    }
    pub fn of(hand: HoleCards) -> Self {
        let [a, b] = hand.cards().map(|c| c.id() as usize);
        Self(a * (103 - a) / 2 + b - a - 1)
    }
    pub const fn get(self) -> usize {
        self.0
    }
    pub fn hand(self) -> HoleCards {
        combos()[self.0]
    }
}
/// Shared canonical combo table; no private cards or range weights are cached.
pub fn combos() -> &'static [HoleCards; COMBO_COUNT] {
    static TABLE: OnceLock<[HoleCards; COMBO_COUNT]> = OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|mut index| {
            for a in 0..51 {
                let row = 51 - a;
                if index < row {
                    return HoleCards::new(
                        Card::from_id(a as u8).unwrap(),
                        Card::from_id((a + 1 + index) as u8).unwrap(),
                    )
                    .unwrap();
                }
                index -= row;
            }
            unreachable!()
        })
    })
}
pub fn compatible(a: HoleCards, b: HoleCards) -> bool {
    a.mask() & b.mask() == 0
}

#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    weights: [f64; COMBO_COUNT],
}
impl Range {
    pub fn empty() -> Self {
        Self {
            weights: [0.0; COMBO_COUNT],
        }
    }
    /// Equal weight 1 on every hand. Normalize explicitly if probabilities needed.
    pub fn uniform() -> Self {
        Self {
            weights: [1.0; COMBO_COUNT],
        }
    }
    pub fn from_weights(weights: [f64; COMBO_COUNT]) -> Result<Self, RangeError> {
        if weights.iter().any(|w| !w.is_finite() || *w < 0.0) {
            return Err(RangeError::InvalidWeight);
        }
        Ok(Self { weights })
    }
    pub fn weights(&self) -> &[f64; COMBO_COUNT] {
        &self.weights
    }
    pub fn weight(&self, hand: HoleCards) -> f64 {
        self.weights[ComboIndex::of(hand).get()]
    }
    pub fn set(&mut self, hand: HoleCards, weight: f64) -> Result<(), RangeError> {
        if !weight.is_finite() || weight < 0.0 {
            return Err(RangeError::InvalidWeight);
        }
        self.weights[ComboIndex::of(hand).get()] = weight;
        Ok(())
    }
    pub fn active_count(&self) -> usize {
        self.weights.iter().filter(|&&w| w > 0.0).count()
    }
    /// Zero hands touching known cards. Accepts board and/or known dead cards;
    /// duplicate known cards are input errors. Does not alter other weights.
    pub fn without_cards(&self, known: &[Card]) -> Result<Self, RangeError> {
        let mask = card_mask(known)?;
        let mut result = self.clone();
        for (i, hand) in combos().iter().enumerate() {
            if hand.mask() & mask != 0 {
                result.weights[i] = 0.0;
            }
        }
        Ok(result)
    }
    /// Normalize using max scaling so valid large relative weights cannot
    /// overflow their sum. Empty ranges remain representable but cannot normalize.
    pub fn normalized(&self) -> Result<Self, RangeError> {
        let max = self.weights.iter().copied().fold(0.0, f64::max);
        if max == 0.0 {
            return Err(RangeError::EmptyRange);
        }
        let total: f64 = self.weights.iter().map(|w| w / max).sum();
        Ok(Self {
            weights: self.weights.map(|w| (w / max) / total),
        })
    }
    /// Opponent distribution conditional on an EXACT known hero hand and board.
    /// Possible cards in a broad hero range are not known dead cards.
    pub fn conditioned_on(&self, board: &Board, hero: HoleCards) -> Result<Self, RangeError> {
        let mut known = board.cards().to_vec();
        known.extend_from_slice(&hero.cards());
        self.without_cards(&known)?.normalized()
    }
}

/// Independent input ranges, conditioned jointly on board and hand compatibility.
/// Prepares O(N^2) sums without storing a dense N-by-N matrix. This is an exact
/// reference implementation (up to f64 arithmetic), not yet a performance kernel.
#[derive(Debug, Clone)]
pub struct JointRanges {
    independent: [Range; 2],
    opponent_mass: [[f64; COMBO_COUNT]; 2],
    compatible_mass: f64,
}
impl JointRanges {
    pub fn new(board: &Board, oop: &Range, ip: &Range) -> Result<Self, RangeError> {
        let independent = [
            oop.without_cards(board.cards())?.normalized()?,
            ip.without_cards(board.cards())?.normalized()?,
        ];
        let mut opponent_mass = [[0.0; COMBO_COUNT]; 2];
        // Direct positive sums avoid cancellation in card-mass inclusion/exclusion.
        for (i, &a) in combos().iter().enumerate() {
            if independent[0].weights[i] == 0.0 {
                continue;
            }
            for (j, &b) in combos().iter().enumerate() {
                if independent[1].weights[j] > 0.0 && compatible(a, b) {
                    opponent_mass[0][i] += independent[1].weights[j];
                    opponent_mass[1][j] += independent[0].weights[i];
                }
            }
        }
        let compatible_mass: f64 = independent[0]
            .weights
            .iter()
            .zip(opponent_mass[0].iter())
            .map(|(w, m)| w * m)
            .sum();
        if compatible_mass == 0.0 {
            return Err(if opponent_mass[0].iter().any(|&m| m > 0.0) {
                RangeError::NumericalUnderflow
            } else {
                RangeError::NoCompatibleHands
            });
        }
        Ok(Self {
            independent,
            opponent_mass,
            compatible_mass,
        })
    }
    /// Normalized, board-filtered independent inputs; NOT joint marginals.
    pub fn independent(&self) -> &[Range; 2] {
        &self.independent
    }
    /// Probability the independent board-filtered draws do not collide.
    pub fn compatible_mass(&self) -> f64 {
        self.compatible_mass
    }
    /// Product of independent weights, with invalid pairs exactly zero.
    /// Sum over all ordered (OOP, IP) pairs is one.
    pub fn probability(&self, oop: HoleCards, ip: HoleCards) -> f64 {
        if !compatible(oop, ip) {
            return 0.0;
        }
        (self.independent[0].weight(oop) * self.independent[1].weight(ip)) / self.compatible_mass
    }
    /// Actual hand marginals AFTER joint card-removal conditioning.
    pub fn marginals(&self) -> [Range; 2] {
        std::array::from_fn(|p| Range {
            weights: std::array::from_fn(|i| {
                self.independent[p].weights[i] * self.opponent_mass[p][i] / self.compatible_mass
            }),
        })
    }
}
