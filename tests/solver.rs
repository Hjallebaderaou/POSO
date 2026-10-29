use poso::{
    betting::{BettingConfig, StreetConfig},
    cards::{Board, HoleCards},
    ranges::Range,
    solver::{Averaging, RiverSolver, SolverError, SolverOptions, StrategyKind},
    tree::{Action, Node, Spot, State},
};
fn board(s: &str) -> Board {
    Board::new(
        &s.split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn hole(s: &str) -> HoleCards {
    let c: Vec<_> = s.split_whitespace().map(|s| s.parse().unwrap()).collect();
    HoleCards::new(c[0], c[1]).unwrap()
}
fn range(entries: &[(&str, f64)]) -> Range {
    let mut r = Range::empty();
    for &(h, w) in entries {
        r.set(hole(h), w).unwrap();
    }
    r
}
fn config(all_in: bool) -> BettingConfig {
    let s = StreetConfig {
        bet_sizes: vec![],
        raise_sizes: vec![],
        include_all_in: all_in,
        max_raises: 0,
    };
    BettingConfig {
        flop: s.clone(),
        turn: s.clone(),
        river: s,
    }
}
fn options() -> SolverOptions {
    SolverOptions {
        averaging: Averaging::Linear,
        averaging_delay: 10,
        max_nodes: 100,
        max_depth: 20,
    }
}
fn root(b: &str) -> State {
    State::new(Spot {
        board: board(b),
        pot: 100,
        stacks: [100, 100],
        minimum_bet: 1,
    })
    .unwrap()
}
fn polarized() -> RiverSolver {
    RiverSolver::new(
        root("2c 3d 7h 9s Jc"),
        [
            range(&[("As Ah", 1.0), ("4s 4h", 1.0)]),
            range(&[("Ks Kh", 1.0)]),
        ],
        &config(true),
        options(),
    )
    .unwrap()
}
fn frequency(s: &RiverSolver, id: usize, h: &str, a: Action) -> f64 {
    let r = s.hand_report(id, hole(h), StrategyKind::Average).unwrap();
    r.frequencies[r.actions.iter().position(|&x| x == a).unwrap()]
}
#[test]
fn check_only_payoffs_match_tree_including_weighted_blockers() {
    let state = root("2c 3d 7h 9s Jc");
    let terminal = state
        .act(Action::Check)
        .unwrap()
        .act(Action::Check)
        .unwrap();
    let ranges = [
        range(&[("As Ah", 2.0), ("Ks Kh", 1.0)]),
        range(&[("As Qs", 3.0), ("Qc Qd", 1.0)]),
    ];
    let joint =
        poso::ranges::JointRanges::new(&board("2c 3d 7h 9s Jc"), &ranges[0], &ranges[1]).unwrap();
    let mut exact = 0.0;
    for a in [hole("As Ah"), hole("Ks Kh")] {
        for b in [hole("As Qs"), hole("Qc Qd")] {
            let p = joint.probability(a, b);
            if p > 0.0 {
                exact += p * terminal.showdown_payoffs_twice([a, b]).unwrap()[0] as f64 / 2.0;
            }
        }
    }
    let mut s = RiverSolver::new(state, ranges, &config(false), options()).unwrap();
    for kind in [StrategyKind::Current, StrategyKind::Average] {
        let v = s.expected_values(kind);
        assert!((v[0] - exact).abs() < 1e-10);
        assert!((v[0] + v[1]).abs() < 1e-10);
    }
    s.solve(20).unwrap();
    assert!((s.expected_values(StrategyKind::Average)[0] - exact).abs() < 1e-10);
}
#[test]
fn board_playing_ties_check_only() {
    let mut s = RiverSolver::new(
        root("As Ks Qs Js Ts"),
        [range(&[("2c 3c", 1.0)]), range(&[("4c 5c", 1.0)])],
        &config(false),
        options(),
    )
    .unwrap();
    s.solve(1).unwrap();
    assert_eq!(s.expected_values(StrategyKind::Current), [0.0, 0.0]);
    assert_eq!(s.expected_values(StrategyKind::Average), [0.0, 0.0]);
}
#[test]
fn polarized_river_recovers_analytic_frequencies_and_value() {
    let mut s = polarized();
    s.solve(2000).unwrap();
    let bet = Action::BetTo(100);
    let root = s.decision(0).unwrap();
    let facing = root.children()[root.actions().iter().position(|&a| a == bet).unwrap()];
    let value = frequency(&s, 0, "As Ah", bet);
    let bluff = frequency(&s, 0, "4s 4h", bet);
    let call = frequency(&s, facing, "Ks Kh", Action::Call);
    eprintln!(
        "value={value:.5} bluff={bluff:.5} call={call:.5} EV={:?}",
        s.expected_values(StrategyKind::Average)
    );
    assert!(value > 0.97);
    assert!((bluff - 0.5).abs() < 0.04);
    assert!((call - 0.5).abs() < 0.04);
    let v = s.expected_values(StrategyKind::Average);
    assert!((v[0] - 25.0).abs() < 1.0);
    assert!((v[0] + v[1]).abs() < 1e-8);
    let report = s
        .hand_report(facing, hole("Ks Kh"), StrategyKind::Average)
        .unwrap();
    for v in report.action_evs.into_iter().flatten() {
        assert!((v + 50.0).abs() < 3.0);
    }
}
#[test]
fn zero_iterations_policy_and_bad_queries() {
    let mut s = polarized();
    s.solve(0).unwrap();
    assert_eq!(s.iterations(), 0);
    let r = s
        .hand_report(0, hole("As Ah"), StrategyKind::Average)
        .unwrap();
    assert_eq!(r.frequencies, [0.5, 0.5]);
    assert!(s.decision(usize::MAX).is_err());
    assert!(s
        .hand_report(0, hole("2c As"), StrategyKind::Average)
        .is_err());
    assert!(s
        .hand_report(0, hole("Qs Qh"), StrategyKind::Current)
        .is_err());
    assert!(s
        .hand_report(999, hole("As Ah"), StrategyKind::Current)
        .is_err());
    assert!(matches!(
        s.decision(0).unwrap().state().node(),
        Node::Player(_)
    ));
}
#[test]
fn tree_limits_wrong_street_and_incompatible_inputs() {
    let mut o = options();
    o.max_nodes = 1;
    assert!(matches!(
        RiverSolver::new(
            root("2c 3d 7h 9s Jc"),
            [Range::uniform(), Range::uniform()],
            &config(true),
            o
        ),
        Err(SolverError::TreeLimit)
    ));
    o = options();
    o.max_depth = 1;
    assert!(matches!(
        RiverSolver::new(
            root("2c 3d 7h 9s Jc"),
            [Range::uniform(), Range::uniform()],
            &config(true),
            o
        ),
        Err(SolverError::TreeLimit)
    ));
    o.max_depth = 257;
    assert!(matches!(
        RiverSolver::new(
            root("2c 3d 7h 9s Jc"),
            [Range::uniform(), Range::uniform()],
            &config(true),
            o
        ),
        Err(SolverError::InvalidOptions)
    ));
    assert!(matches!(
        RiverSolver::new(
            root("2c 3d 7h"),
            [Range::uniform(), Range::uniform()],
            &config(true),
            options()
        ),
        Err(SolverError::NotRiver)
    ));
    assert!(matches!(
        RiverSolver::new(
            root("2c 3d 7h 9s Jc"),
            [range(&[("As Ah", 1.0)]), range(&[("As Ks", 1.0)])],
            &config(true),
            options()
        ),
        Err(SolverError::Ranges(_))
    ));
}
#[test]
fn full_ranges_one_iteration_finite_normalized_and_zero_sum() {
    let mut s = RiverSolver::new(
        root("2c 3d 7h 9s Jc"),
        [Range::uniform(), Range::uniform()],
        &config(true),
        options(),
    )
    .unwrap();
    s.solve(1).unwrap();
    for kind in [StrategyKind::Current, StrategyKind::Average] {
        let v = s.expected_values(kind);
        assert!(v.iter().all(|v| v.is_finite()));
        assert!((v[0] + v[1]).abs() < 1e-7);
        for h in ["As Ah", "4s 4h", "Ks Qs"] {
            let r = s.hand_report(0, hole(h), kind).unwrap();
            assert!((r.frequencies.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            assert!(r.frequencies.iter().all(|&p| (0.0..=1.0).contains(&p)));
            assert!(r.action_evs.iter().flatten().all(|v| v.is_finite()));
        }
    }
}
#[test]
fn resumable_iterations_and_uniform_average() {
    let mut a = polarized();
    let mut b = polarized();
    a.solve(12).unwrap();
    a.solve(13).unwrap();
    b.solve(25).unwrap();
    assert_eq!(a.iterations(), 25);
    assert_eq!(
        a.expected_values(StrategyKind::Average),
        b.expected_values(StrategyKind::Average)
    );
    assert_eq!(
        a.hand_report(0, hole("4s 4h"), StrategyKind::Average)
            .unwrap()
            .frequencies,
        b.hand_report(0, hole("4s 4h"), StrategyKind::Average)
            .unwrap()
            .frequencies
    );
    let mut o = options();
    o.averaging = Averaging::Uniform;
    o.averaging_delay = 0;
    let mut s = RiverSolver::new(
        root("2c 3d 7h 9s Jc"),
        [range(&[("As Ah", 1.0)]), range(&[("Ks Kh", 1.0)])],
        &config(true),
        o,
    )
    .unwrap();
    s.solve(20).unwrap();
    assert!(
        (s.expected_values(StrategyKind::Average)[0] + s.expected_values(StrategyKind::Average)[1])
            .abs()
            < 1e-10
    );
}

#[test]
fn asymmetric_raise_tree_initial_ev_matches_scalar_pair_enumeration() {
    use poso::betting::{PotFraction, RaiseBasis, RaiseSize};
    let b = "2c 3d 7h 9s Jc";
    let root = State::new(Spot {
        board: board(b),
        pot: 101,
        stacks: [90, 135],
        minimum_bet: 10,
    })
    .unwrap();
    let street = StreetConfig {
        bet_sizes: vec![PotFraction::new(1, 3).unwrap()],
        raise_sizes: vec![RaiseSize {
            fraction: PotFraction::new(1, 2).unwrap(),
            basis: RaiseBasis::PotAfterCall,
        }],
        include_all_in: true,
        max_raises: 1,
    };
    let config = BettingConfig {
        flop: street.clone(),
        turn: street.clone(),
        river: street,
    };
    let a = range(&[("As Ah", 2.0), ("4s 4h", 1.0), ("Ks Qs", 3.0)]);
    let other = range(&[("As Qs", 2.0), ("Ks Kh", 1.0), ("4c 4d", 4.0)]);
    let joint = poso::ranges::JointRanges::new(&board(b), &a, &other).unwrap();
    // Independent scalar reference: fix both private hands, enumerate all public
    // actions uniformly, evaluate each terminal through the tree's payoff API.
    fn scalar(state: State, h: [HoleCards; 2], config: &BettingConfig) -> f64 {
        match state.node() {
            Node::Terminal(poso::tree::Terminal::Fold { .. }) => {
                state.fold_payoffs_twice().unwrap()[0] as f64 / 2.0
            }
            Node::Terminal(poso::tree::Terminal::Showdown) => {
                state.showdown_payoffs_twice(h).unwrap()[0] as f64 / 2.0
            }
            Node::Player(_) => {
                let actions = config.actions(&state).unwrap();
                actions
                    .iter()
                    .map(|&act| scalar(config.act(&state, act).unwrap(), h, config))
                    .sum::<f64>()
                    / actions.len() as f64
            }
            Node::Chance => unreachable!(),
        }
    }
    let mut exact = 0.0;
    for h in [hole("As Ah"), hole("4s 4h"), hole("Ks Qs")] {
        for k in [hole("As Qs"), hole("Ks Kh"), hole("4c 4d")] {
            let p = joint.probability(h, k);
            if p > 0.0 {
                exact += p * scalar(root.clone(), [h, k], &config);
            }
        }
    }
    let mut s = RiverSolver::new(root, [a, other], &config, options()).unwrap();
    let ev = s.expected_values(StrategyKind::Current);
    assert!((ev[0] - exact).abs() < 1e-10);
    assert!((ev[0] + ev[1]).abs() < 1e-10);
    s.solve(25).unwrap();
    let ev = s.expected_values(StrategyKind::Average);
    assert!((ev[0] + ev[1]).abs() < 1e-9);
}

#[test]
fn batch_reports_match_per_hand_queries_and_track_fallback() {
    use poso::ranges::ComboIndex;
    let mut s = polarized();
    let h = hole("As Ah");
    let initial = s.node_reports(0).unwrap();
    assert!(!initial[ComboIndex::of(h).get()].as_ref().unwrap().1);
    assert!(initial[ComboIndex::of(hole("2c As")).get()].is_none());
    s.solve(40).unwrap();
    let bet = s
        .decision(0)
        .unwrap()
        .actions()
        .iter()
        .position(|a| *a == Action::BetTo(100))
        .unwrap();
    let facing = s.decision(0).unwrap().children()[bet];
    for (id, hands) in [(0, vec!["As Ah", "4s 4h"]), (facing, vec!["Ks Kh"])] {
        let batch = s.node_reports(id).unwrap();
        for hand in hands {
            let h = hole(hand);
            let (report, accumulated) = batch[ComboIndex::of(h).get()].as_ref().unwrap();
            let scalar = s.hand_report(id, h, StrategyKind::Average).unwrap();
            assert!(*accumulated);
            assert_eq!(report.actions, scalar.actions);
            assert_eq!(report.frequencies, scalar.frequencies);
            assert_eq!(report.action_evs, scalar.action_evs);
        }
    }
}
