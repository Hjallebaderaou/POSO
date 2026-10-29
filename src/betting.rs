//! Configurable action abstraction. Poker legality remains in `tree::State`.
//! Use `BettingConfig::act` (not raw `State::act`) to enforce the abstraction.
use crate::tree::{Action, Node, State, Street, TreeError};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BettingError {
    InvalidFraction,
    Tree(TreeError),
    ActionNotConfigured,
}
impl From<TreeError> for BettingError {
    fn from(e: TreeError) -> Self {
        Self::Tree(e)
    }
}
impl fmt::Display for BettingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for BettingError {}

/// Positive exact pot fraction: 33% = 33/100, one third = 1/3.
/// Integer arithmetic avoids binary floating-point rounding ambiguity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotFraction {
    numerator: u32,
    denominator: u32,
}
impl PotFraction {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, BettingError> {
        if numerator == 0 || denominator == 0 {
            return Err(BettingError::InvalidFraction);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
    pub fn numerator(self) -> u32 {
        self.numerator
    }
    pub fn denominator(self) -> u32 {
        self.denominator
    }
    /// Nearest integer, half up. The caller's base is at most twice u64::MAX.
    fn chips(self, base: u128) -> u128 {
        (base * u128::from(self.numerator) + u128::from(self.denominator / 2))
            / u128::from(self.denominator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiseBasis {
    /// Increment above highest commitment = fraction * (current pot + to-call).
    PotAfterCall,
    /// Increment above highest commitment = fraction * current pot.
    CurrentPot,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaiseSize {
    pub fraction: PotFraction,
    pub basis: RaiseBasis,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreetConfig {
    pub bet_sizes: Vec<PotFraction>,
    /// Can mix both sizing conventions in the same street.
    pub raise_sizes: Vec<RaiseSize>,
    pub include_all_in: bool,
    /// Raises AFTER the opening bet. Strict, including all-in raises.
    pub max_raises: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BettingConfig {
    pub flop: StreetConfig,
    pub turn: StreetConfig,
    pub river: StreetConfig,
}
impl BettingConfig {
    pub fn for_street(&self, street: Street) -> &StreetConfig {
        match street {
            Street::Flop => &self.flop,
            Street::Turn => &self.turn,
            Street::River => &self.river,
        }
    }
    /// Check first when free; otherwise fold then call. Aggressive actions follow
    /// in ascending commitment order, with equivalent totals deduplicated.
    /// Candidates below the minimum are omitted, not silently raised to minimum.
    /// Oversized candidates cap at the actor's stack even if explicit all-in is off.
    pub fn actions(&self, state: &State) -> Result<Vec<Action>, BettingError> {
        let player = match state.node() {
            Node::Player(p) => p,
            _ => return Err(TreeError::WrongNode.into()),
        };
        let config = self.for_street(state.street().ok_or(TreeError::WrongNode)?);
        let call = state.to_call()?;
        let mut actions = if call == 0 {
            vec![Action::Check]
        } else {
            vec![Action::Fold, Action::Call]
        };
        let committed = state.committed();
        let high = committed[0].max(committed[1]);
        let opening = high == 0;
        if !opening && state.raises_this_street() >= config.max_raises {
            return Ok(actions);
        }
        let max_total = committed[player.index()] + state.stacks()[player.index()];
        let pot = u128::from(state.pot());
        let mut totals = Vec::new();
        if opening {
            for &size in &config.bet_sizes {
                totals.push(size.chips(pot));
            }
        } else {
            for size in &config.raise_sizes {
                let base = match size.basis {
                    RaiseBasis::PotAfterCall => pot + u128::from(call),
                    RaiseBasis::CurrentPot => pot,
                };
                totals.push(u128::from(high) + size.fraction.chips(base));
            }
        }
        if config.include_all_in {
            totals.push(u128::from(max_total));
        }
        let mut totals: Vec<u64> = totals
            .into_iter()
            .map(|total| total.min(u128::from(max_total)) as u64)
            .collect();
        totals.sort_unstable();
        totals.dedup();
        for total in totals {
            let action = if opening {
                Action::BetTo(total)
            } else {
                Action::RaiseTo(total)
            };
            if state.act(action).is_ok() {
                actions.push(action);
            }
        }
        Ok(actions)
    }
    /// Validate both abstraction membership (including raise caps) and poker rules.
    pub fn act(&self, state: &State, action: Action) -> Result<State, BettingError> {
        if !self.actions(state)?.contains(&action) {
            return Err(BettingError::ActionNotConfigured);
        }
        Ok(state.act(action)?)
    }
}
