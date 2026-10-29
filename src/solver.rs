//! Deterministic, alternating CFR+ on a finite abstracted heads-up river tree.
//! Vectors always use the exact 1,326-combo indexing; no buckets or sampling.
use crate::{
    betting::{BettingConfig, BettingError},
    cards::{Board, CardError, HoleCards},
    evaluator::evaluate,
    ranges::{combos, ComboIndex, JointRanges, Range, RangeError, COMBO_COUNT},
    tree::{Action, Node, Player, State, Street, Terminal, TreeError},
};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverError {
    NotRiver,
    InvalidOptions,
    TreeLimit,
    InvalidNode,
    UnsupportedHand,
    IterationOverflow,
    Betting(BettingError),
    Cards(CardError),
    Ranges(RangeError),
    Tree(TreeError),
}
macro_rules! from_error {
    ($ty:ty, $variant:ident) => {
        impl From<$ty> for SolverError {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(BettingError, Betting);
from_error!(CardError, Cards);
from_error!(RangeError, Ranges);
from_error!(TreeError, Tree);
impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for SolverError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Averaging {
    Uniform,
    Linear,
}
#[derive(Debug, Clone, Copy)]
pub struct SolverOptions {
    pub averaging: Averaging,
    /// First `averaging_delay` complete iterations do not enter the average.
    pub averaging_delay: u64,
    pub max_nodes: usize,
    /// Root is depth 0. Limited to 256 to bound recursion stack usage.
    pub max_depth: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyKind {
    Current,
    Average,
}
#[derive(Debug, Clone)]
pub struct Decision {
    state: State,
    actions: Vec<Action>,
    children: Vec<usize>,
    // Action-major, then combo. Each public-history node has its own arrays.
    regrets: Vec<f64>,
    strategy_sum: Vec<f64>,
}
impl Decision {
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
    pub fn children(&self) -> &[usize] {
        &self.children
    }
}
#[derive(Debug, Clone)]
enum Entry {
    Pending,
    Decision(Decision),
    // OOP payoff for OOP loss, tie, win. Fold uses the same value in all slots.
    Leaf([f64; 3]),
}
#[derive(Debug, Clone)]
pub struct HandReport {
    pub actions: Vec<Action>,
    pub frequencies: Vec<f64>,
    /// Conditional chip EV after each action; None if this information set has
    /// zero opponent reach under the requested policy.
    pub action_evs: Vec<Option<f64>>,
}

pub struct RiverSolver {
    nodes: Vec<Entry>,
    options: SolverOptions,
    iterations: u64,
    joint: JointRanges,
    active: [Vec<usize>; 2],
    // OOP-vs-IP outcome: -1 lose, 0 tie, 1 win, 2 incompatible.
    outcomes: Vec<i8>,
}
impl RiverSolver {
    pub fn new(
        root: State,
        ranges: [Range; 2],
        config: &BettingConfig,
        options: SolverOptions,
    ) -> Result<Self, SolverError> {
        if root.street() != Some(Street::River) || matches!(root.node(), Node::Chance) {
            return Err(SolverError::NotRiver);
        }
        if options.max_nodes == 0 || options.max_depth == 0 || options.max_depth > 256 {
            return Err(SolverError::InvalidOptions);
        }
        let board = Board::new(root.board())?;
        let joint = JointRanges::new(&board, &ranges[0], &ranges[1])?;
        let marginals = joint.marginals();
        let active = std::array::from_fn(|p| {
            (0..COMBO_COUNT)
                .filter(|&i| marginals[p].weights()[i] > 0.0)
                .collect()
        });
        let ranks: Vec<_> = combos()
            .iter()
            .map(|h| {
                let mut c = root.board().to_vec();
                c.extend_from_slice(&h.cards());
                evaluate(&c).ok()
            })
            .collect();
        let mut outcomes = vec![2; COMBO_COUNT * COMBO_COUNT];
        for (i, a) in combos().iter().enumerate() {
            for (j, b) in combos().iter().enumerate() {
                if a.mask() & b.mask() == 0 {
                    if let (Some(x), Some(y)) = (ranks[i], ranks[j]) {
                        outcomes[i * COMBO_COUNT + j] = match x.cmp(&y) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                    }
                }
            }
        }
        let mut solver = Self {
            nodes: Vec::new(),
            options,
            iterations: 0,
            joint,
            active,
            outcomes,
        };
        solver.build(root, config, 0)?;
        Ok(solver)
    }
    fn build(
        &mut self,
        state: State,
        config: &BettingConfig,
        depth: usize,
    ) -> Result<usize, SolverError> {
        if self.nodes.len() >= self.options.max_nodes || depth > self.options.max_depth {
            return Err(SolverError::TreeLimit);
        }
        let id = self.nodes.len();
        self.nodes.push(Entry::Pending);
        self.nodes[id] = match state.node() {
            Node::Terminal(Terminal::Fold { .. }) => {
                let v = state.fold_payoffs_twice()?[0] as f64 / 2.0;
                Entry::Leaf([v; 3])
            }
            Node::Terminal(Terminal::Showdown) => {
                let loss = state.showdown_outcome_payoffs_twice(Some(Player::Ip))?[0] as f64 / 2.0;
                let tie = state.showdown_outcome_payoffs_twice(None)?[0] as f64 / 2.0;
                let win = state.showdown_outcome_payoffs_twice(Some(Player::Oop))?[0] as f64 / 2.0;
                Entry::Leaf([loss, tie, win])
            }
            Node::Player(_) => {
                let actions = config.actions(&state)?;
                let mut children = Vec::new();
                for &a in &actions {
                    children.push(self.build(config.act(&state, a)?, config, depth + 1)?);
                }
                let n = actions.len() * COMBO_COUNT;
                Entry::Decision(Decision {
                    state,
                    actions,
                    children,
                    regrets: vec![0.0; n],
                    strategy_sum: vec![0.0; n],
                })
            }
            Node::Chance => return Err(SolverError::NotRiver),
        };
        Ok(id)
    }
    pub fn iterations(&self) -> u64 {
        self.iterations
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn decision(&self, id: usize) -> Result<&Decision, SolverError> {
        match self.nodes.get(id) {
            Some(Entry::Decision(d)) => Ok(d),
            _ => Err(SolverError::InvalidNode),
        }
    }
    fn policy(&self, id: usize, kind: StrategyKind) -> Vec<f64> {
        let d = self.decision(id).expect("internal decision ID");
        let source = match kind {
            StrategyKind::Current => &d.regrets,
            StrategyKind::Average => &d.strategy_sum,
        };
        let mut policy = vec![0.0; source.len()];
        for h in 0..COMBO_COUNT {
            let sum: f64 = (0..d.actions.len())
                .map(|a| source[a * COMBO_COUNT + h])
                .sum();
            for a in 0..d.actions.len() {
                policy[a * COMBO_COUNT + h] = if sum > 0.0 {
                    source[a * COMBO_COUNT + h] / sum
                } else {
                    1.0 / d.actions.len() as f64
                };
            }
        }
        policy
    }
    fn terminal_values(&self, values: [f64; 3], player: usize, opp: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; COMBO_COUNT];
        for &h in &self.active[player] {
            for &k in &self.active[1 - player] {
                if opp[k] == 0.0 {
                    continue;
                }
                let outcome = if player == 0 {
                    self.outcomes[h * COMBO_COUNT + k]
                } else {
                    self.outcomes[k * COMBO_COUNT + h]
                };
                if outcome != 2 {
                    let v = values[(outcome + 1) as usize];
                    out[h] += opp[k] * if player == 0 { v } else { -v };
                }
            }
        }
        out
    }
    // Values exclude own reach: opponent-only counterfactual values. Own range
    // weights and joint Z are constants for each information set and are omitted
    // from regrets, but included in reported root EV normalization.
    fn update(&mut self, id: usize, player: usize, opp: &[f64]) -> Vec<f64> {
        if let Entry::Leaf(v) = self.nodes[id] {
            return self.terminal_values(v, player, opp);
        }
        let d = self.decision(id).unwrap();
        let actor = match d.state.node() {
            Node::Player(p) => p.index(),
            _ => unreachable!(),
        };
        let children = d.children.clone();
        let policy = self.policy(id, StrategyKind::Current);
        let mut result = vec![0.0; COMBO_COUNT];
        if actor == player {
            let mut action_values = Vec::new();
            for (a, &child) in children.iter().enumerate() {
                let v = self.update(child, player, opp);
                for &h in &self.active[player] {
                    result[h] += policy[a * COMBO_COUNT + h] * v[h];
                }
                action_values.push(v);
            }
            let Entry::Decision(d) = &mut self.nodes[id] else {
                unreachable!()
            };
            for (a, v) in action_values.iter().enumerate() {
                for &h in &self.active[player] {
                    let index = a * COMBO_COUNT + h;
                    d.regrets[index] = (d.regrets[index] + v[h] - result[h]).max(0.0);
                }
            }
        } else {
            for (a, &child) in children.iter().enumerate() {
                let reach: Vec<_> = opp
                    .iter()
                    .enumerate()
                    .map(|(h, r)| r * policy[a * COMBO_COUNT + h])
                    .collect();
                let v = self.update(child, player, &reach);
                for &h in &self.active[player] {
                    result[h] += v[h];
                }
            }
        }
        result
    }
    fn accumulate(&mut self, id: usize, reach: [Vec<f64>; 2], weight: f64) {
        if !matches!(self.nodes[id], Entry::Decision(_)) {
            return;
        }
        let d = self.decision(id).unwrap();
        let actor = match d.state.node() {
            Node::Player(p) => p.index(),
            _ => unreachable!(),
        };
        let children = d.children.clone();
        let policy = self.policy(id, StrategyKind::Current);
        let Entry::Decision(d) = &mut self.nodes[id] else {
            unreachable!()
        };
        for a in 0..children.len() {
            for &h in &self.active[actor] {
                d.strategy_sum[a * COMBO_COUNT + h] +=
                    weight * reach[actor][h] * policy[a * COMBO_COUNT + h];
            }
        }
        for (a, &child) in children.iter().enumerate() {
            let mut next = reach.clone();
            for &h in &self.active[actor] {
                next[actor][h] *= policy[a * COMBO_COUNT + h];
            }
            self.accumulate(child, next, weight);
        }
    }
    /// One iteration = OOP update, IP update using updated OOP, then snapshot
    /// accumulation of BOTH players' post-update policies with own reach only.
    pub fn solve(&mut self, iterations: u64) -> Result<(), SolverError> {
        self.iterations
            .checked_add(iterations)
            .ok_or(SolverError::IterationOverflow)?;
        for _ in 0..iterations {
            for player in 0..2 {
                let opp = self.joint.independent()[1 - player].weights().to_vec();
                self.update(0, player, &opp);
            }
            self.iterations += 1;
            if self.iterations > self.options.averaging_delay {
                let weight = match self.options.averaging {
                    Averaging::Uniform => 1.0,
                    Averaging::Linear => (self.iterations - self.options.averaging_delay) as f64,
                };
                self.accumulate(0, [vec![1.0; COMBO_COUNT], vec![1.0; COMBO_COUNT]], weight);
            }
        }
        Ok(())
    }
    fn values(&self, id: usize, player: usize, opp: &[f64], kind: StrategyKind) -> Vec<f64> {
        if let Entry::Leaf(v) = self.nodes[id] {
            return self.terminal_values(v, player, opp);
        }
        let d = self.decision(id).unwrap();
        let actor = match d.state.node() {
            Node::Player(p) => p.index(),
            _ => unreachable!(),
        };
        let policy = self.policy(id, kind);
        let mut result = vec![0.0; COMBO_COUNT];
        for (a, &child) in d.children.iter().enumerate() {
            let reach = if actor == player {
                opp.to_vec()
            } else {
                opp.iter()
                    .enumerate()
                    .map(|(h, r)| r * policy[a * COMBO_COUNT + h])
                    .collect()
            };
            let v = self.values(child, player, &reach, kind);
            for &h in &self.active[player] {
                result[h] += if actor == player {
                    policy[a * COMBO_COUNT + h] * v[h]
                } else {
                    v[h]
                };
            }
        }
        result
    }
    /// Expected zero-sum utility, in chips (not twice chips), baseline P/2 sunk.
    pub fn expected_values(&self, kind: StrategyKind) -> [f64; 2] {
        std::array::from_fn(|p| {
            let v = self.values(0, p, self.joint.independent()[1 - p].weights(), kind);
            self.active[p]
                .iter()
                .map(|&h| self.joint.independent()[p].weights()[h] * v[h])
                .sum::<f64>()
                / self.joint.compatible_mass()
        })
    }
    fn path(&self, target: usize) -> Option<Vec<(usize, usize)>> {
        fn find(nodes: &[Entry], id: usize, target: usize, path: &mut Vec<(usize, usize)>) -> bool {
            if id == target {
                return true;
            }
            if let Entry::Decision(d) = &nodes[id] {
                for (a, &child) in d.children.iter().enumerate() {
                    path.push((id, a));
                    if find(nodes, child, target, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            false
        }
        let mut path = Vec::new();
        find(&self.nodes, 0, target, &mut path).then_some(path)
    }
    pub fn hand_report(
        &self,
        id: usize,
        hand: HoleCards,
        kind: StrategyKind,
    ) -> Result<HandReport, SolverError> {
        let d = self.decision(id)?;
        let actor = match d.state.node() {
            Node::Player(p) => p.index(),
            _ => unreachable!(),
        };
        let h = ComboIndex::of(hand).get();
        if !self.active[actor].contains(&h) {
            return Err(SolverError::UnsupportedHand);
        }
        let policy = self.policy(id, kind);
        let mut opp = self.joint.independent()[1 - actor].weights().to_vec();
        for (ancestor, a) in self.path(id).ok_or(SolverError::InvalidNode)? {
            let node = self.decision(ancestor)?;
            if node.state.node() == Node::Player(if actor == 0 { Player::Ip } else { Player::Oop })
            {
                let p = self.policy(ancestor, kind);
                for (k, r) in opp.iter_mut().enumerate() {
                    *r *= p[a * COMBO_COUNT + k];
                }
            }
        }
        let denominator: f64 = self.active[1 - actor]
            .iter()
            .filter(|&&k| {
                let outcome = if actor == 0 {
                    self.outcomes[h * COMBO_COUNT + k]
                } else {
                    self.outcomes[k * COMBO_COUNT + h]
                };
                outcome != 2
            })
            .map(|&k| opp[k])
            .sum();
        let action_evs = d
            .children
            .iter()
            .map(|&child| {
                if denominator > 0.0 {
                    Some(self.values(child, actor, &opp, kind)[h] / denominator)
                } else {
                    None
                }
            })
            .collect();
        Ok(HandReport {
            actions: d.actions.clone(),
            frequencies: (0..d.actions.len())
                .map(|a| policy[a * COMBO_COUNT + h])
                .collect(),
            action_evs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regret_matching_plus_clamp_and_own_reach_averaging() {
        // The integration tests exercise the actual update; pin the regret
        // matching formula here as well, including uniform zero-regret fallback.
        let config = crate::betting::StreetConfig {
            bet_sizes: vec![],
            raise_sizes: vec![],
            include_all_in: true,
            max_raises: 0,
        };
        let config = BettingConfig {
            flop: config.clone(),
            turn: config.clone(),
            river: config,
        };
        let board = Board::new(
            &"2c 3d 7h 9s Jc"
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let root = State::new(crate::tree::Spot {
            board,
            pot: 100,
            stacks: [100, 100],
            minimum_bet: 1,
        })
        .unwrap();
        let mut s = RiverSolver::new(
            root,
            [Range::uniform(), Range::uniform()],
            &config,
            SolverOptions {
                averaging: Averaging::Uniform,
                averaging_delay: 0,
                max_nodes: 100,
                max_depth: 20,
            },
        )
        .unwrap();
        assert_eq!(s.policy(0, StrategyKind::Current)[0], 0.5);
        let Entry::Decision(d) = &mut s.nodes[0] else {
            unreachable!()
        };
        d.regrets[0] = 1.0;
        d.regrets[COMBO_COUNT] = 3.0;
        assert_eq!(s.policy(0, StrategyKind::Current)[0], 0.25);
        assert_eq!(s.policy(0, StrategyKind::Current)[COMBO_COUNT], 0.75);

        // With zero regrets, every decision is uniform. OOP's downstream
        // response to IP's bet has own reach 1/2, not joint reach 1/4.
        let Entry::Decision(d) = &mut s.nodes[0] else {
            unreachable!()
        };
        d.regrets.fill(0.0);
        s.accumulate(0, [vec![1.0; COMBO_COUNT], vec![1.0; COMBO_COUNT]], 2.0);
        let h =
            ComboIndex::of(HoleCards::new("As".parse().unwrap(), "Ah".parse().unwrap()).unwrap())
                .get();
        let after_check = s.decision(0).unwrap().children[0];
        let after_check_bet = s.decision(after_check).unwrap().children[1];
        assert_eq!(s.decision(0).unwrap().strategy_sum[h], 1.0);
        assert_eq!(s.decision(after_check).unwrap().strategy_sum[h], 1.0);
        assert_eq!(s.decision(after_check_bet).unwrap().strategy_sum[h], 0.5);
        s.solve(2).unwrap();
        let mut positive = false;
        let mut zero = false;
        for node in &s.nodes {
            if let Entry::Decision(d) = node {
                for &regret in &d.regrets {
                    assert!(regret.is_finite() && regret >= 0.0);
                    positive |= regret > 0.0;
                    zero |= regret == 0.0;
                }
            }
        }
        assert!(positive && zero);
        // A history with no opponent reach must report undefined conditional EV,
        // not zero EV or NaN. Force OOP to check every hand.
        let facing_bet = s.decision(0).unwrap().children[1];
        let Entry::Decision(d) = &mut s.nodes[0] else {
            unreachable!()
        };
        d.regrets.fill(0.0);
        d.regrets[..COMBO_COUNT].fill(1.0);
        let report = s
            .hand_report(
                facing_bet,
                HoleCards::new("Ks".parse().unwrap(), "Kh".parse().unwrap()).unwrap(),
                StrategyKind::Current,
            )
            .unwrap();
        assert!(report.action_evs.iter().all(Option::is_none));
        s.iterations = u64::MAX;
        assert_eq!(s.solve(1), Err(SolverError::IterationOverflow));
    }
}
