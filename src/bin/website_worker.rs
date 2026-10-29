//! Local website subprocess protocol. All inputs are validated here, even if the
//! HTTP layer is bypassed. One process owns at most one bounded solver.
use poso::{
    betting::{BettingConfig, PotFraction, RaiseBasis, RaiseSize, StreetConfig},
    cards::{Board, Card},
    ranges::{Range, COMBO_COUNT},
    solver::{Averaging, RiverSolver, SolverOptions, StrategyKind},
    tree::{Action, Node, Spot, State},
};
use std::io::{self, BufRead, Write};

fn line<R: BufRead>(r: &mut R) -> Result<String, String> {
    let mut s = String::new();
    if r.read_line(&mut s).map_err(|e| e.to_string())? == 0 {
        return Err("Unexpected end of input".into());
    }
    if s.len() > 100_000 {
        return Err("Input too large".into());
    }
    Ok(s.trim().to_string())
}
fn num<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("Invalid number: {s}"))
}
fn nums(s: &str) -> Result<Vec<u64>, String> {
    s.split_whitespace().map(num).collect()
}
fn emit(s: &str) {
    println!("{s}");
    let _ = io::stdout().flush();
}
fn safe_error(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}
fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let mut r = stdin.lock();
    let board_line = line(&mut r)?;
    let cards = board_line
        .split_whitespace()
        .map(|s| s.parse::<Card>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if cards.len() != 5 {
        return Err("Exactly five river cards are required".into());
    }
    let board = Board::new(&cards).map_err(|e| e.to_string())?;
    let settings = nums(&line(&mut r)?)?;
    if settings.len() != 9 {
        return Err("Invalid settings".into());
    }
    let [pot, oop, ip, minbet, iters, delay, linear, allin, cap]: [u64; 9] =
        settings.try_into().unwrap();
    if pot == 0
        || pot > 1_000_000
        || oop == 0
        || oop > 1_000_000
        || ip == 0
        || ip > 1_000_000
        || minbet == 0
        || minbet > 1_000_000
        || iters == 0
        || iters > 5000
        || delay >= iters
        || linear > 1
        || allin > 1
        || cap > 2
    {
        return Err("Settings outside allowed limits (iterations 1–5000; delay must be smaller; amounts 1–1,000,000; raise cap 0–2)".into());
    }
    let bets = nums(&line(&mut r)?)?;
    let raises = nums(&line(&mut r)?)?;
    let bases = nums(&line(&mut r)?)?;
    if bets.len() > 3
        || raises.len() > 3
        || raises.len() != bases.len()
        || bets.iter().chain(&raises).any(|&x| x == 0 || x > 500)
        || bases.iter().any(|&x| x > 1)
    {
        return Err("Use at most three sizes per type, between 1% and 500%".into());
    }
    let fraction = |n: u64| PotFraction::new(n as u32, 100).unwrap();
    let street = StreetConfig {
        bet_sizes: bets.into_iter().map(fraction).collect(),
        raise_sizes: raises
            .into_iter()
            .zip(bases)
            .map(|(n, b)| RaiseSize {
                fraction: fraction(n),
                basis: if b == 0 {
                    RaiseBasis::PotAfterCall
                } else {
                    RaiseBasis::CurrentPot
                },
            })
            .collect(),
        include_all_in: allin == 1,
        max_raises: cap,
    };
    let cfg = BettingConfig {
        flop: street.clone(),
        turn: street.clone(),
        river: street,
    };
    let mut ranges = Vec::new();
    let mut active = [0usize; 2];
    for count in &mut active {
        let weights = line(&mut r)?
            .split_whitespace()
            .map(num::<f64>)
            .collect::<Result<Vec<_>, _>>()?;
        if weights.len() != COMBO_COUNT
            || weights
                .iter()
                .any(|w| !w.is_finite() || *w < 0.0 || *w > 1.0)
        {
            return Err("Each range must contain 1,326 weights between 0 and 1".into());
        }
        let array: [f64; COMBO_COUNT] = weights.try_into().unwrap();
        let range = Range::from_weights(array).map_err(|e| e.to_string())?;
        *count = range
            .without_cards(board.cards())
            .map_err(|e| e.to_string())?
            .active_count();
        ranges.push(range);
    }
    let state = State::new(Spot {
        board,
        pot,
        stacks: [oop, ip],
        minimum_bet: minbet,
    })
    .map_err(|e| e.to_string())?;
    let mut solver = RiverSolver::new(
        state,
        [ranges.remove(0), ranges.remove(0)],
        &cfg,
        SolverOptions {
            averaging: if linear == 1 {
                Averaging::Linear
            } else {
                Averaging::Uniform
            },
            averaging_delay: delay,
            max_nodes: 100,
            max_depth: 16,
        },
    )
    .map_err(|e| format!("Cannot build solver: {e}"))?;
    // Conservative operation budget; active-pair terminal loops dominate cost.
    if (active[0] as u64)
        .saturating_mul(active[1] as u64)
        .saturating_mul(solver.node_count() as u64)
        .saturating_mul(iters)
        > 150_000_000
    {
        return Err("Compute limit exceeded. Reduce ranges, iterations or betting sizes.".into());
    }
    while solver.iterations() < iters {
        solver
            .solve((iters - solver.iterations()).min(10))
            .map_err(|e| e.to_string())?;
        emit(&format!(
            "{{\"type\":\"progress\",\"iterations\":{}}}",
            solver.iterations()
        ));
    }
    let ev = solver.expected_values(StrategyKind::Average);
    emit(&format!(
        "{{\"type\":\"ready\",\"iterations\":{},\"nodes\":{},\"ev\":[{},{}]}}",
        solver.iterations(),
        solver.node_count(),
        ev[0],
        ev[1]
    ));
    loop {
        let cmd = line(&mut r)?;
        if cmd == "quit" {
            break;
        }
        let id = cmd
            .strip_prefix("node ")
            .ok_or("Invalid command")?
            .parse::<usize>()
            .map_err(|_| "Invalid node")?;
        let d = match solver.decision(id) {
            Ok(d) => d,
            Err(_) => {
                emit("{\"type\":\"terminal\"}");
                continue;
            }
        };
        let actor = match d.state().node() {
            Node::Player(p) => p.index(),
            _ => unreachable!(),
        };
        let actions = d
            .actions()
            .iter()
            .zip(d.children())
            .map(|(a, c)| {
                format!(
                    "{{\"label\":\"{}\",\"child\":{c}}}",
                    match a {
                        Action::Check => "Check".into(),
                        Action::Fold => "Fold".into(),
                        Action::Call => "Call".into(),
                        Action::BetTo(n) => format!("Bet to {n}"),
                        Action::RaiseTo(n) => format!("Raise to {n}"),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let reports = solver
            .node_reports(id)
            .map_err(|e| e.to_string())?
            .iter()
            .map(|r| match r {
                None => "null".into(),
                Some((r, acc)) => format!(
                    "{{\"frequencies\":[{}],\"evs\":[{}],\"accumulated\":{acc}}}",
                    r.frequencies
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    r.action_evs
                        .iter()
                        .map(|x| x.map_or("null".into(), |v| v.to_string()))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            })
            .collect::<Vec<String>>()
            .join(",");
        emit(&format!("{{\"type\":\"node\",\"id\":{id},\"actor\":{actor},\"pot\":{},\"actions\":[{actions}],\"reports\":[{reports}]}}",d.state().pot()));
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        emit(&format!(
            "{{\"type\":\"error\",\"message\":\"{}\"}}",
            safe_error(&e)
        ));
        std::process::exit(1)
    }
}
