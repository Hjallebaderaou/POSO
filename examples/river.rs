//! Run with: cargo run --release --example river -- 2000
use poso::{
    betting::{BettingConfig, StreetConfig},
    cards::{Board, HoleCards},
    ranges::Range,
    solver::{Averaging, RiverSolver, SolverOptions, StrategyKind},
    tree::{Action, Spot, State},
};
fn hand(a: &str, b: &str) -> HoleCards {
    HoleCards::new(a.parse().unwrap(), b.parse().unwrap()).unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = std::env::args()
        .nth(1)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(2000u64);
    let board = Board::new(
        &"2c 3d 7h 9s Jc"
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let state = State::new(Spot {
        board,
        pot: 100,
        stacks: [100, 100],
        minimum_bet: 1,
    })?;
    let mut oop = Range::empty();
    oop.set(hand("As", "Ah"), 1.0)?;
    oop.set(hand("4s", "4h"), 1.0)?;
    let mut ip = Range::empty();
    ip.set(hand("Ks", "Kh"), 1.0)?;
    let street = StreetConfig {
        bet_sizes: vec![],
        raise_sizes: vec![],
        include_all_in: true,
        max_raises: 0,
    };
    let config = BettingConfig {
        flop: street.clone(),
        turn: street.clone(),
        river: street,
    };
    let mut solver = RiverSolver::new(
        state,
        [oop, ip],
        &config,
        SolverOptions {
            averaging: Averaging::Linear,
            averaging_delay: 10,
            max_nodes: 100,
            max_depth: 20,
        },
    )?;
    println!("Actual NLHE river: 2c 3d 7h 9s Jc; pot=100 stacks=100/100");
    println!("OOP AsAh/4s4h equally weighted; IP KsKh. Check/all-in, no raises.");
    while solver.iterations() < iterations {
        solver.solve((iterations - solver.iterations()).min(500))?;
        let ev = solver.expected_values(StrategyKind::Average);
        println!(
            "iterations={} average EV OOP={:.5} IP={:.5}",
            solver.iterations(),
            ev[0],
            ev[1]
        );
    }
    for (label, h) in [("AsAh", hand("As", "Ah")), ("4s4h", hand("4s", "4h"))] {
        let report = solver.hand_report(0, h, StrategyKind::Average)?;
        println!("OOP {label}: {report:?}");
    }
    let root = solver.decision(0)?;
    let index = root
        .actions()
        .iter()
        .position(|&a| a == Action::BetTo(100))
        .unwrap();
    println!(
        "IP KsKh facing bet: {:?}",
        solver.hand_report(
            root.children()[index],
            hand("Ks", "Kh"),
            StrategyKind::Average
        )?
    );
    println!("Analytic target: value bet=1, bluff bet=0.5, call=0.5; OOP EV=25 chips.");
    println!("Exploitability is NOT measured yet (Step 6). This is a backend test harness, not the final web interface.");
    Ok(())
}
