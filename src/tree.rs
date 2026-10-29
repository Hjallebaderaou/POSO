//! Lazy public extensive-form Hold'em tree. No private cards live in a node.
//! OOP (player 0) acts first on every postflop street. Amounts are integer units.
use crate::cards::{card_mask, Board, Card, CardError, HoleCards};
use crate::evaluator::evaluate;
use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    Oop,
    Ip,
}
impl Player {
    pub const fn index(self) -> usize {
        match self {
            Self::Oop => 0,
            Self::Ip => 1,
        }
    }
    pub const fn other(self) -> Self {
        match self {
            Self::Oop => Self::Ip,
            Self::Ip => Self::Oop,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Street {
    Flop,
    Turn,
    River,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Check,
    Fold,
    Call,
    /// Total commitment on this street, NOT the additional chips paid now.
    BetTo(u64),
    RaiseTo(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Fold { winner: Player },
    Showdown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Player(Player),
    /// One card at a time; flop's three ordered draws represent one unordered flop.
    Chance,
    Terminal(Terminal),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    Cards(CardError),
    InvalidConfig,
    IllegalAction,
    WrongNode,
}
impl From<CardError> for TreeError {
    fn from(e: CardError) -> Self {
        Self::Cards(e)
    }
}
impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for TreeError {}

/// Root spot: pot is already in the middle, stacks are chips BEHIND.
/// The initial pot is split equally as sunk contribution for zero-sum utility.
/// Empty board starts at flop chance (not a preflop betting round).
#[derive(Debug, Clone)]
pub struct Spot {
    pub board: Board,
    pub pot: u64,
    pub stacks: [u64; 2],
    pub minimum_bet: u64,
}

#[derive(Debug, Clone)]
pub struct State {
    board: Vec<Card>,
    initial_pot: u64,
    stacks: [u64; 2],
    invested: [u64; 2],
    committed: [u64; 2],
    minimum_bet: u64,
    last_full_raise: u64,
    raises_this_street: u64,
    acted: [bool; 2],
    node: Node,
}
impl State {
    pub fn new(spot: Spot) -> Result<Self, TreeError> {
        if spot.minimum_bet == 0
            || spot
                .pot
                .checked_add(spot.stacks[0])
                .and_then(|n| n.checked_add(spot.stacks[1]))
                .is_none()
        {
            return Err(TreeError::InvalidConfig);
        }
        let mut s = Self {
            board: spot.board.cards().to_vec(),
            initial_pot: spot.pot,
            stacks: spot.stacks,
            invested: [0; 2],
            committed: [0; 2],
            minimum_bet: spot.minimum_bet,
            last_full_raise: spot.minimum_bet,
            raises_this_street: 0,
            acted: [false; 2],
            node: Node::Player(Player::Oop),
        };
        if s.board.is_empty() {
            s.node = Node::Chance;
        } else if s.stacks.contains(&0) {
            s.finish_round();
        }
        Ok(s)
    }
    pub fn node(&self) -> Node {
        self.node
    }
    /// May have 1 or 2 cards during ordered flop chance draws.
    pub fn board(&self) -> &[Card] {
        &self.board
    }
    pub fn street(&self) -> Option<Street> {
        match self.board.len() {
            3 => Some(Street::Flop),
            4 => Some(Street::Turn),
            5 => Some(Street::River),
            _ => None,
        }
    }
    pub fn pot(&self) -> u64 {
        self.initial_pot + self.invested[0] + self.invested[1]
    }
    pub fn stacks(&self) -> [u64; 2] {
        self.stacks
    }
    pub fn committed(&self) -> [u64; 2] {
        self.committed
    }
    /// Raises after the opening bet, including short all-in raises.
    pub fn raises_this_street(&self) -> u64 {
        self.raises_this_street
    }
    pub fn to_call(&self) -> Result<u64, TreeError> {
        let p = self.actor()?.index();
        Ok(self.committed[1 - p].saturating_sub(self.committed[p]))
    }
    fn actor(&self) -> Result<Player, TreeError> {
        match self.node {
            Node::Player(p) => Ok(p),
            _ => Err(TreeError::WrongNode),
        }
    }
    /// Private information-set key: public history/path is supplied by traversal,
    /// plus ONLY the acting player's hole cards. Opponent cards are not an input.
    pub fn information_key(
        &self,
        public_path: u64,
        own: HoleCards,
    ) -> Result<InformationKey, TreeError> {
        let player = self.actor()?;
        if own.mask() & card_mask(&self.board)? != 0 {
            return Err(TreeError::Cards(CardError::DuplicateCard));
        }
        Ok(InformationKey {
            public_path,
            player,
            own,
        })
    }
    /// Transition without changing this state. Bet-size abstraction enumerates
    /// candidate totals separately; every candidate must pass this rule engine.
    pub fn act(&self, action: Action) -> Result<Self, TreeError> {
        let player = self.actor()?;
        let p = player.index();
        let o = 1 - p;
        let call = self.to_call()?;
        let mut next = self.clone();
        match action {
            Action::Fold => {
                if call == 0 {
                    return Err(TreeError::IllegalAction);
                }
                next.refund_uncalled();
                next.node = Node::Terminal(Terminal::Fold {
                    winner: player.other(),
                });
                return Ok(next);
            }
            Action::Check => {
                if call != 0 {
                    return Err(TreeError::IllegalAction);
                }
                next.acted[p] = true;
            }
            Action::Call => {
                if call == 0 {
                    return Err(TreeError::IllegalAction);
                }
                next.pay(p, call.min(next.stacks[p]));
                next.acted[p] = true;
                if next.stacks.contains(&0) {
                    next.refund_uncalled();
                    next.finish_round();
                    return Ok(next);
                }
            }
            Action::BetTo(total) | Action::RaiseTo(total) => {
                let high = self.committed[0].max(self.committed[1]);
                let opening = high == 0;
                if opening != matches!(action, Action::BetTo(_))
                    || self.stacks[o] == 0
                    || (!opening && self.acted[p])
                    || total <= high
                    || total <= self.committed[p]
                    || total - self.committed[p] > self.stacks[p]
                {
                    return Err(TreeError::IllegalAction);
                }
                let increment = total - high;
                let full = increment >= self.last_full_raise;
                let all_in = total - self.committed[p] == self.stacks[p];
                if !full && !all_in {
                    return Err(TreeError::IllegalAction);
                }
                next.pay(p, total - self.committed[p]);
                if !opening {
                    next.raises_this_street += 1;
                }
                // An opening short all-in still permits a player who checked to
                // respond. A short RAISE does not reopen a prior bettor's action.
                if full || opening {
                    next.acted = [false; 2];
                }
                if full {
                    next.last_full_raise = increment;
                }
                next.acted[p] = true;
            }
        }
        if next.acted.iter().all(|&a| a) && next.committed[0] == next.committed[1] {
            next.finish_round();
        } else {
            next.node = Node::Player(player.other());
        }
        Ok(next)
    }
    fn pay(&mut self, p: usize, amount: u64) {
        self.stacks[p] -= amount;
        self.invested[p] += amount;
        self.committed[p] += amount;
    }
    fn refund_uncalled(&mut self) {
        let p = if self.committed[0] > self.committed[1] {
            0
        } else {
            1
        };
        let refund = self.committed[p] - self.committed[1 - p];
        self.committed[p] -= refund;
        self.invested[p] -= refund;
        self.stacks[p] += refund;
    }
    fn finish_round(&mut self) {
        self.committed = [0; 2];
        self.acted = [false; 2];
        self.last_full_raise = self.minimum_bet;
        self.raises_this_street = 0;
        self.node = if self.board.len() == 5 {
            Node::Terminal(Terminal::Showdown)
        } else {
            Node::Chance
        };
    }
    /// Public branch enumeration. Conditional private-card probability is exposed
    /// separately: uniformly weighting this list is NOT correct for fixed hands.
    pub fn possible_cards(&self) -> Result<Vec<Card>, TreeError> {
        if self.node != Node::Chance {
            return Err(TreeError::WrongNode);
        }
        let mask = card_mask(&self.board)?;
        Ok((0..52)
            .map(|i| Card::from_id(i).unwrap())
            .filter(|c| c.mask() & mask == 0)
            .collect())
    }
    /// Physical deal probabilities conditional on a compatible pair of hands.
    /// Solver traversal must use blockers and these denominators for each pair.
    pub fn conditional_deals(&self, hands: [HoleCards; 2]) -> Result<Vec<(Card, f64)>, TreeError> {
        if self.node != Node::Chance {
            return Err(TreeError::WrongNode);
        }
        let blocked = self.validate_hands(hands)?;
        let n = 52 - blocked.count_ones();
        Ok(self
            .possible_cards()?
            .into_iter()
            .filter(|c| c.mask() & blocked == 0)
            .map(|c| (c, 1.0 / n as f64))
            .collect())
    }
    pub fn deal(&self, card: Card) -> Result<Self, TreeError> {
        if self.node != Node::Chance {
            return Err(TreeError::WrongNode);
        }
        if card.mask() & card_mask(&self.board)? != 0 {
            return Err(TreeError::Cards(CardError::DuplicateCard));
        }
        let mut next = self.clone();
        next.board.push(card);
        if next.board.len() >= 3 {
            if next.stacks.contains(&0) {
                next.finish_round();
            } else {
                next.node = Node::Player(Player::Oop);
            }
        }
        Ok(next)
    }
    fn validate_hands(&self, hands: [HoleCards; 2]) -> Result<u64, TreeError> {
        let mut all = self.board.clone();
        all.extend_from_slice(&hands[0].cards());
        all.extend_from_slice(&hands[1].cards());
        Ok(card_mask(&all)?)
    }
    /// Twice the net zero-sum payoff, relative to half the initial pot plus
    /// each player's later contributions. Exact even when the pot is odd.
    /// Divide by 2.0 at the numerical solver boundary. This returns fractional
    /// chip expectations for ties, not casino-specific odd-chip distribution.
    pub fn fold_payoffs_twice(&self) -> Result<[i128; 2], TreeError> {
        match self.node {
            Node::Terminal(Terminal::Fold { winner }) => Ok(self.payoffs(Some(winner))),
            _ => Err(TreeError::WrongNode),
        }
    }
    pub fn showdown_payoffs_twice(&self, hands: [HoleCards; 2]) -> Result<[i128; 2], TreeError> {
        if self.node != Node::Terminal(Terminal::Showdown) {
            return Err(TreeError::WrongNode);
        }
        self.validate_hands(hands)?;
        let strength = |h: HoleCards| {
            let mut cards = self.board.clone();
            cards.extend_from_slice(&h.cards());
            evaluate(&cards)
        };
        use std::cmp::Ordering;
        let winner = match strength(hands[0])?.cmp(&strength(hands[1])?) {
            Ordering::Greater => Some(Player::Oop),
            Ordering::Less => Some(Player::Ip),
            Ordering::Equal => None,
        };
        Ok(self.payoffs(winner))
    }
    // Solver supplies outcomes from a board-specific, blocker-checked rank table.
    pub(crate) fn showdown_outcome_payoffs_twice(
        &self,
        winner: Option<Player>,
    ) -> Result<[i128; 2], TreeError> {
        if self.node != Node::Terminal(Terminal::Showdown) {
            return Err(TreeError::WrongNode);
        }
        Ok(self.payoffs(winner))
    }
    fn payoffs(&self, winner: Option<Player>) -> [i128; 2] {
        let pot = self.pot() as i128;
        std::array::from_fn(|p| {
            let receipt = match winner {
                Some(w) if w.index() == p => 2 * pot,
                Some(_) => 0,
                None => pot,
            };
            receipt - self.initial_pot as i128 - 2 * self.invested[p] as i128
        })
    }
}
/// `public_path` must uniquely identify a public action/deal history in a tree.
/// It is not a hash of the state: histories with equal pots remain distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InformationKey {
    pub public_path: u64,
    pub player: Player,
    pub own: HoleCards,
}
