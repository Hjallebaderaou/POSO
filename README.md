# POSO — heads-up No-Limit Texas Hold'em solver

Incremental Rust implementation, correctness before performance. **Steps 1–5 are implemented, including the first river CFR+ solver.
Best-response exploitability is not implemented. A local-only river website is included.**
Rust provides safe compact representations and contiguous numerical arrays without
requiring a garbage collector. This crate currently has no dependencies.

## Structure

```text
Cargo.toml
src/
  lib.rs
  cards.rs        # one-byte cards, canonical hole cards, validated public boards
  evaluator.rs    # direct exact 5–7-card evaluation
  tree.rs         # public states, rules, chance transitions, exact payoffs
  betting.rs      # per-street sizes, both raise formulas, strict raise caps
  ranges.rs       # 1,326-combo weights, dead cards, joint card removal
  solver.rs       # river tree compilation, CFR+, strategies and EV queries
  bin/website_worker.rs # local website solver subprocess
web/              # local server, interface and tests
tests/
  evaluator.rs    # independent oracle, edge cases, exhaustive distribution
  tree.rs         # rules, transitions, payoffs, blockers, small tree enumeration
  betting.rs      # sizing, rounding, caps, legality and abstract tree enumeration
  ranges.rs       # combo bijection, weighted blockers, joint probability oracles
  solver.rs       # analytic river spot, scalar EV oracle, full-range smoke test
examples/
  river.rs        # executable backend milestone demo
```

Planned solver work includes a separate `analysis` module for best responses.

## Local website

This interface runs on your computer, not on the internet. Install a stable Rust
toolchain (1.88 or later), Cargo, and Node.js 18 or later. No npm packages or
external service are required.

```sh
npm start
# Open http://localhost:3000 in your browser; Ctrl+C stops the server.
```

`npm start` builds the native solver worker in release mode before serving the
site. To change the port on macOS/Linux, run `PORT=3001 npm start`; on PowerShell,
run `$env:PORT=3001; npm start`. The production/local launch is the same: there
is intentionally no public deployment. To build and start separately:

```sh
cargo build --release --bin website_worker
node web/server.js
```

The server binds **only 127.0.0.1**. It validates the Host header and requires a
same-origin JSON POST for starting solves; it does not enable CORS. Do not proxy
or expose it to the network: it is a local tool, not an authenticated public API.
It serves static HTML/CSS/JavaScript and launches the real Rust solver in one
separate subprocess, leaving the browser responsive. The subprocess owns one
solver; a new solve replaces a completed one, while concurrent solves are
rejected. Closing the terminal stops the server. There is no cloud service,
account, stored job history, tracking or deployment configuration.

### Using the workspace

* Set five unique river cards, the initial pot, both stacks behind, the minimum
  bet, and chips per BB. BB is a display conversion only; solver amounts are
  integer chips. OOP acts first.
* Choose OOP or IP and paint matrix cells with a weight between 0% and 100%.
  Upper triangle = suited, lower triangle = offsuit, diagonal = pairs. Painting
  sets every exact combo in that cell. Fill/Clear affect the selected range.
  The exact-combo editor replaces a range, e.g. `AsAh:100, 4s4h:50`, and can
  show the current weights. Board-blocked combinations remain editable but
  are removed from the solve.
* Enter up to three integer percentage sizes each for bets and raises, select
  the raise basis, all-in candidate, and strict raise cap. Empty sizes are
  permitted. Minimum illegal actions are omitted, and configured sizes can
  still cap at all-in when the separate all-in candidate is disabled. See
  **Step 3** below for the exact formulas and rounding conventions.
* Choose 1–5,000 iterations, uniform or linear averaging, and a delay smaller
  than the iteration count. Run the solve. The default/preset is the existing
  polarized river example. Progress counts completed iterations, not estimated
  time or convergence. On large ranges, reduce sizes or iterations if the
  conservative workload limit rejects the spot.
* Use action buttons and breadcrumbs to navigate actual solver node IDs;
  terminal nodes have no strategy. Select a cell and exact combo for action
  frequencies and EVs. The result matrix averages each action frequency using
  the acting player's **input combo weights**, not opponent-adjusted marginals
  or path reach weights. Board-blocked and joint-unsupported hands, hands with
  no accumulated average policy, and hands with zero compatible opponent reach
  are excluded. Empty cells do not mean a zero-frequency strategy. Exact hand
  detail distinguishes blocked/unsupported, no accumulation and unreachable.
* EVs are conditional continuation values in chips, plus a BB display conversion.
  They include the original initial-pot/2 sunk baseline; a numeric zero is a
  genuine zero EV, not missing data. Root EVs use the joint blocker-correct
  distribution. Exploitability and best responses are **not implemented**;
  iteration count alone is not a convergence guarantee. Only heads-up river
  solving is supported: no preflop, flop, or turn solving.

The Rust boundary revalidates cards, weights, settings and sizes. Limits are
100 nodes, depth 16, 1–5,000 iterations, up to three bet and raise sizes (1–500%),
raise cap 0–2, chip inputs 1–1,000,000, and weights 0–1. A conservative budget
of 150 million (board-unblocked OOP combos × IP combos × nodes × iterations)
rejects large requests before traversal; the local server also kills a solve
after 120 seconds. Tree/action limits bound allocated solver arrays but are not
an OS-level memory quota. There is one worker process at a time. The website
never sends poker inputs to an external service.

```sh
cargo build --release --bin website_worker  # required before the web API tests
cargo test
npm test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Run tests

Install a stable Rust toolchain (tested with Rust 1.88.0; edition 2021), then:

```sh
cargo test
cargo test --release -- --ignored   # all 2,598,960 five-card combinations
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The normal suite checks all nine hand categories, every tie-breaker position via
oracle comparison, wheel straights, two trips, three pairs, seven-card flushes,
playing the board, duplicate rejection, parsing, 10,000 deterministic random
5/6/7-card hands against an independent best-of-five-subsets oracle, and all 24
global suit permutations on each sampled hand. The ignored exhaustive test
checks every five-card hand against the oracle, the nine known category totals
and the 7,462 distinct strengths. Royal flush is an ace-high straight flush, not
a separate category. Suits never break ties.

**Validation:** tested with Rust 1.88.0. The normal Rust suite and the exhaustive five-card oracle/distribution test pass. `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` pass. README API examples are also
compiled as crate documentation tests.

The sandbox initially had no Rust toolchain and Debian installation was blocked.
For this session, Rust components were installed from the `@rustbin` npm mirror
into `/home/user/.cache/poso-rust/toolchain` (outside Git). To use that temporary
installation, prefix commands with:

```sh
export PATH="/home/user/.cache/poso-rust/toolchain/bin:$PATH"
```

This mirror is only an environment workaround, not a project dependency or a
recommended production toolchain source; use your normal trusted Rust toolchain
locally. Toolchain/cache files are intentionally not included in the repository.

## API example

```rust
use poso::{cards::Card, evaluator::{evaluate, Category}};
let hand: Vec<Card> = "As Ks Qs Js Ts 2d 3c"
    .split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
let strength = evaluate(&hand)?;
assert_eq!(strength.category(), Category::StraightFlush);
# Ok::<(), poso::cards::CardError>(())
```

Card ID = `4 * rank + suit`, with ranks `2..A` and suits `c,d,h,s`.
A card occupies one byte; a card set fits in 52 bits of a `u64`. Hole cards are
canonical unordered pairs. Boards accept 0, 3, 4 or 5 unique cards. Combining
board and hole cards must still be validated together by `evaluate`/`card_mask`.
Notation is two ASCII characters (`Td`, not `10d`), case insensitive.

### Evaluator method

The evaluator builds 13 rank counts and four 13-bit suit masks in one pass.
It tests categories strongest-first. Straights are five consecutive bits with
an explicit ace-low wheel check. Full houses use the highest trips followed by
the best *other* pair/trips; three-pair hands use the third pair as a possible
kicker. Flushes select the five highest ranks from their suit only. With at most
seven cards, at most one suit can have five cards.

`HandValue` packs the category and up to five lexicographically ordered rank
nibbles into a `u32`. Larger values win, equal values split the pot. Unused
nibbles are zero. The production path uses bounded loops, no allocation, no
lookup-table startup, and no enumeration of the 21 five-card subsets. This is a
standard rank-count/bitmask approach, not a benchmarked claim of table-evaluator
performance. The test-only oracle independently sorts five-card rank groups and
enumerates subsets for six/seven cards.

## Roadmap and decision gates

1. **Cards/evaluation:** implemented; normal and exhaustive five-card tests pass.
2. **Public extensive-form tree:** implemented as lazy transitions; nine tree
   tests pass. Public nodes exclude private cards.
3. **Betting abstraction:** implemented; both raise formulas, omitted undersized
   actions, and strict raise caps confirmed with the user. Nine betting tests pass.
4. **Ranges/blockers:** implemented with 11 range tests; stable combo indexing,
   weighted vectors, dead-card filtering and normalized compatible joint draws.
5. **River milestone:** implemented; alternating full-range CFR+, configurable
   delay/uniform or linear averaging, per-hand strategies and action EV queries.
   Analytic polarized spot and full-range traversal tests pass.
6. Exact best responses against the average strategy, compatible-hand joint
   normalization, documented exploitability convention as % of initial pot,
   convergence reports and analytic polarized/bluff-catcher tests.
7. Turn/flop chance traversal, card-removal-correct probabilities, and suit
   isomorphism only when board **and weighted ranges** preserve the symmetry.
   Measure memory before/after; asymmetric ranges cannot be merged blindly.
8. Profile first; then compact storage and deterministic threaded traversal.
   Quantization must be validated for numerical/convergence error. MCCFR/GPU optional.
9. Preflop clustering: much larger street/runout/action space; measure abstraction
   error and explain how bucket boundaries and postflop values connect.
10. Local website: per-hand/action strategy and EV, weighted 13x13 aggregates,
    tree navigation, bet sizing and iteration count. Exploitability remains unimplemented.

Each step gets tests and review before proceeding. No simplified poker variants.

## Known limitations / next step

No best responses, exploitability metric, benchmarks or threading yet. Steps 1–5 compile and pass their supplied tests.
No preflop betting, blinds, antes, rake, side pots, or multiway support. Amounts
are integer units; a future display layer will configure units per BB. Showdown
ties split expected value exactly, rather than using seat-dependent odd chips.
No exhaustive seven-card check (133,784,560 combinations) is included.

Next: Step 6 — exact best responses, exploitability and convergence validation. Later extensions could include subgame solving and depth-limited
value functions. More-player solving is outside this heads-up scope.

## Step 2 — public game tree

The user confirmed **no rake** and **integer chip units**. `Spot` specifies a
validated board, current pot, stacks *behind*, and the minimum opening bet.
Player 0 is OOP and acts first on each postflop street. An empty board begins
flop chance, not a preflop betting round. Roots start with no outstanding bet.

`State::act` returns a new public child state; `node()` distinguishes player,
chance, fold and showdown nodes. This lazy representation avoids materializing
an enormous unabstracted tree. Candidate bet sizes and raise caps are added in
Step 3, while this module validates each candidate's actual poker legality:
check/call/fold, bet-to vs raise-to totals, full minimum raises, short all-ins,
non-reopening short raises, and no betting against an all-in opponent.
Uncalled chips are refunded on folds and unequal-stack all-in calls. Matched
all-ins run out remaining board cards without further player actions.

```rust
use poso::cards::Board;
use poso::tree::{Action, Spot, State};
let board = "2c 3d 7h 9s Jc".split_whitespace()
    .map(str::parse).collect::<Result<Vec<_>, _>>()?;
let root = State::new(Spot {
    board: Board::new(&board)?, pot: 100, stacks: [200, 200], minimum_bet: 10,
})?;
let child = root.act(Action::BetTo(75))?.act(Action::Call)?;
assert_eq!(child.pot(), 250);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Information and chance

No hole cards are stored in public states. `InformationKey` combines a caller-
assigned unique public-history path ID, acting player, and **only their own**
canonical hole cards. The traversal must assign collision-free path IDs; pots
and boards alone are not history keys. Strategy/regret arrays are not built yet.
A test checks own-hand and history separation; solver-level imperfect-information
correctness still needs validation when those arrays are implemented.

`possible_cards()` lists public branches without knowing private hands.
**Do not assign a uniform public probability to fixed private hands.**
`conditional_deals([oop_hand, ip_hand])` validates all blockers and returns exact
uniform probabilities over cards outside the board and those four hole cards:
45 turn possibilities and 44 river possibilities after a flop. Vectorized
traversal will account for this separately for each compatible hand pair.
`deal(card)` validates public-board duplication only; the caller must assign
zero reach to private hands blocked by the card.

Flop chance uses three ordered single-card draws. Each unordered flop occurs
six times with equal weight; this is exact but deliberately redundant until
chance compression/suit isomorphism in Step 7. One/two-card boards occur only
inside this chance sequence and cannot be supplied as root `Board`s.

### Exact utility convention

Let initial pot be P, subsequent contributions c0/c1, and final pot
T = P+c0+c1. Each player's baseline sunk share is P/2. Utilities are:

- Winner i: T - P/2 - ci.
- Loser i: -P/2 - ci.
- Tie: T/2 - P/2 - ci.

These are zero-sum. Returned `i128` values are **twice** utility, so odd initial
pots remain exact without floats. Divide by 2 at the solver boundary. If a
later interface wants EV excluding sunk pot share, add P/2 to displayed values;
this constant does not change strategies. Fold utilities need no private cards;
showdown utilities validate both private hands against each other and the board,
then use the Step 1 evaluator. Stacks and total chip arithmetic are bounded at
construction to prevent overflow.

```sh
cargo test --test tree
cargo test
cargo test --release -- --ignored
```

Tree tests cover all street transitions, ordered flop chance, conditional deal
probabilities and blockers, full/short raises, all-in runouts, refunds, odd pots,
showdown wins/ties, wrong-node operations, information-key separation, and an
enumerated small real-Hold'em river tree with chip-conservation and zero-sum
checks at every leaf. All nine tree tests pass in the current sandbox.


## Step 3 — configurable betting abstraction

`BettingConfig` contains separate `StreetConfig`s for flop, turn, and river.
Each provides opening pot fractions, raise sizes, an explicit-all-in switch, and
`max_raises`. Pot and behind-stack sizes remain configured by `Spot`.
No default sizing policy is silently imposed.

The confirmed conventions are:

- **Both raise formulas are supported**, selectable per `RaiseSize`, even mixed
  within one street. If H is the highest street commitment, P the current pot
  including outstanding bets, C the amount to call, and f the configured fraction:
  - `PotAfterCall`: raise-to = H + round(f × (P + C)).
  - `CurrentPot`: raise-to = H + round(f × P).
- Opening bet-to = round(f × P). Fractions are positive exact `u32` ratios:
  `33/100` means 33%, not one third. Rounding is nearest unit, halves up.
  `u128` intermediate arithmetic protects extreme chip amounts and ratios.
- Undersized actions are **omitted**, not increased to a minimum. A legal short
  all-in remains available if explicitly enabled or reached by a configured size.
- Oversized totals cap at the acting player's actual all-in commitment. Equal
  totals are merged, including totals from different formulas. The switch
  `include_all_in = false` suppresses the extra all-in candidate; it does **not**
  forbid configured fractions from reaching/capping at all-in.
- Raise caps count **raises after the opening bet**, including short all-in
  raises. Cap 0 permits bets but no raises. All-in raises obey the cap. Counters
  reset at the end of each street. Check, fold and call are never removed by caps.
- Heads-up bets can exceed the opponent's stack; the existing tree refunds the
  unmatched amount after calling/folding. This avoids introducing a distinct
  effective-stack sizing convention silently.

For a pot of 100 facing a bet of 50, a 75% raise becomes raise-to **200** using
`PotAfterCall`, or **163** using `CurrentPot` (112.5 rounds to 113).

```rust
use poso::betting::{BettingConfig, PotFraction, RaiseBasis, RaiseSize, StreetConfig};
use poso::cards::Board;
use poso::tree::{Action, Spot, State};
let f = PotFraction::new(75, 100)?;
let street = StreetConfig {
    bet_sizes: vec![PotFraction::new(33, 100)?, f, PotFraction::new(150, 100)?],
    raise_sizes: vec![
        RaiseSize { fraction: f, basis: RaiseBasis::PotAfterCall },
        RaiseSize { fraction: f, basis: RaiseBasis::CurrentPot },
    ],
    include_all_in: true,
    max_raises: 1,
};
let config = BettingConfig {
    flop: street.clone(), turn: street.clone(), river: street,
};
let board = "2c 3d 7h 9s Jc".split_whitespace()
    .map(str::parse).collect::<Result<Vec<_>, _>>()?;
let root = State::new(Spot {
    board: Board::new(&board)?, pot: 100, stacks: [1000, 1000], minimum_bet: 10,
})?;
assert_eq!(config.actions(&root)?, vec![
    Action::Check, Action::BetTo(33), Action::BetTo(75),
    Action::BetTo(150), Action::BetTo(1000),
]);
let child = config.act(&root, Action::BetTo(75))?;
assert_eq!(child.pot(), 175);
# Ok::<(), Box<dyn std::error::Error>>(())
```

**Use `BettingConfig::actions` and `BettingConfig::act` for abstracted trees.**
Raw `State::act` intentionally enforces poker rules only, not configured sizes
or caps. It remains useful to validate external/manual actions, but does not
make an off-tree action part of the abstraction. Non-player nodes return a
wrong-node error; the caller handles chance and terminal nodes separately.

Returned action ordering is deterministic: check (or fold then call), followed
by aggressive actions in ascending total commitment order. Only legal unique
actions are returned. Empty size lists allow check-only or call/fold-only
subgames; there is no implicit fallback minimum bet.

```sh
cargo test --test betting
cargo test
cargo test --release -- --ignored
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Nine new betting tests cover both formulas, exact fractions and half-up rounding,
street-specific settings, raise caps/reset, omission vs short all-ins, duplicate
merging, stack capping, off-abstraction rejection, zero pots, empty size lists,
all-in opponents, `u64` boundary amounts, and enumeration of a small abstracted
river tree. Current limitations: one shared sizing policy per street (not yet
per-player or depth-specific), no JSON/UI configuration parser, and no automatic
near-all-in merging other than exact total equality. The local website consumes this API.


## Step 4 — ranges and blockers

`Range` stores a contiguous `[f64; 1326]` in a stable order. Card IDs are the
rank-major IDs from Step 1. Hole-card pairs `(a,b)` with `a < b` are ordered
lexicographically, with index `a * (103-a) / 2 + b-a-1`. `ComboIndex` validates
integer indices and converts both ways; `combos()` shares the immutable table.
The table includes all 78 pair combos, 312 suited combos and 936 offsuit combos.

Weights are **finite nonnegative relative masses**, not necessarily probabilities
or limited to 1. They may encode frequencies, but they do not encode hand
correlations between players. Construction/setters reject negative, NaN and
infinite inputs. No mutable slice is exposed that could bypass validation.
`empty`, `uniform`, `from_weights`, `set`, `weight`, `weights` and `active_count`
provide the basic vector API. A vector occupies 10,608 bytes of weight storage.

- `without_cards(known)` returns a copy with overlapping combos exactly zero.
  Other weights are unchanged. Known cards may include a board and extra dead
  cards; duplicate known cards are rejected. No automatic normalization occurs.
- `normalized()` explicitly returns probability weights. Max scaling avoids
  overflow even when all 1,326 inputs are `f64::MAX`. A zero range is representable
  but cannot normalize (`EmptyRange`).
- `conditioned_on(board, hero_hand)` filters the board and a **known exact** hero
  hand, then normalizes the opponent range. Hero/board collisions are errors.
- `JointRanges::new(board, oop, ip)` filters the board from each input and
  normalizes each independently. It then conditions their product distribution
  on the hands sharing no card. It does not modify the input ranges.

For board-filtered independent weights `w0`, `w1`, define
`Z = sum(w0[h] * w1[k])` over compatible pairs. Then:

```text
Pr(h,k | board, compatible) = w0[h] * w1[k] / Z   if compatible
                           = 0                  otherwise
```

`probability` exposes that joint distribution and `marginals()` sums it for each
player. `independent()` returns the board-filtered independent inputs, which are
**not** the same as those marginals. A hand with no compatible opponent support
gets marginal zero, but the mere possibility of the opponent holding one of its
cards does not globally delete it. Empty input support and no compatible joint
support are separate errors. `compatible_mass()` returns Z for the independently
normalized inputs, not the raw unnormalized sum.

```rust
use poso::cards::{Board, HoleCards};
use poso::ranges::{JointRanges, Range};
let board = "2c 3d 7h 9s Jc".split_whitespace()
    .map(str::parse).collect::<Result<Vec<_>, _>>()?;
let board = Board::new(&board)?;
let hero = HoleCards::new("As".parse()?, "Ah".parse()?)?;
let opponent = Range::uniform().conditioned_on(&board, hero)?;
assert_eq!(opponent.active_count(), 990); // choose(45, 2)
let both = JointRanges::new(&board, &Range::uniform(), &Range::uniform())?;
let villain = HoleCards::new("Ks".parse()?, "Kh".parse()?)?;
assert!((both.probability(hero, villain) - 1.0 / (1081.0 * 990.0)).abs() < 1e-12);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Correctness and cost

There are 1,176 available combos on a flop, 1,128 on a turn and 1,081 on a river
before conditioning on a private hand. A river hero hand leaves 990 possible
opponent combos. The tests exhaustively check the combo-index bijection and
all uniform-range river pair probabilities, compare weighted joint probabilities
against a separate raw-product oracle, and check normalization, weighted
marginals, player swapping, scale invariance, invalid inputs and blocker counts.

Joint preparation currently uses direct positive sums over O(1326²) pairs and
O(1326) stored vectors, **not** a dense joint matrix. Direct sums avoid numerical
cancellation from subtracting card masses. This is a correctness-first reference;
profiling and faster blocker kernels come later. `JointRanges` describes the
initial compatible deal distribution, not updated strategy-dependent reaches.
The solver must propagate its own reach vectors without renormalizing at every
node. A uniform product of the two separately normalized marginals would be wrong.

All computations use `f64`: very small weights/products can underflow and very
large relative dynamic ranges are not lossless. If compatible support exists but
the entire joint normalizer underflows, construction returns `NumericalUnderflow`
rather than incorrectly reporting impossible hands or returning NaNs. Individual
vanishingly small terms can still round to zero; arbitrary-precision/log-domain
weights are not implemented. The website supports matrix painting and exact-combo text input, but not a shorthand range parser (`AKs`, `QQ+`, etc.).

```sh
cargo test --test ranges
cargo test
cargo test --release -- --ignored
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```


## Step 5 — first working river CFR+ solver

`RiverSolver::new` takes a river `State`, two exact-combo ranges, a
`BettingConfig`, and explicit `SolverOptions`. Non-river boards are rejected.
The public river tree is materialized with stable history-node IDs and explicit
node/depth limits. Each decision stores action-major regret and strategy-sum
arrays indexed by **only the acting player's hand**, not the opponent's hand.
The root node ID is 0; `decision(id)` exposes its state, actions and child IDs for
navigation. A terminal-root river spot can be evaluated but has no decisions.

### Traversal and update convention

One complete iteration performs:

1. OOP counterfactual regret update under current strategies.
2. IP counterfactual regret update using OOP's updated strategies.
3. Separate own-reach-weighted accumulation of **both post-update policies**.

At each information set, regret matching uses positive cumulative regrets, with
a uniform policy when their sum is zero. Each regret update is
`R[a,h] = max(0, R[a,h] + value[a,h] - value[h])` (CFR+). Opponent decisions
multiply opponent reach vectors; traverser decisions compute all action values
and a per-hand policy-weighted baseline. No private deals are sampled, no hands
are bucketed, and there is no per-opponent-hand policy.

Traversal vectors and decision storage use the full 1,326-combo indexing. Zero
joint-support hands are skipped in arithmetic; every supported combo is visited.
"Vectorized" here means range-vector traversal, **not** hardware SIMD or GPU.
The terminal kernel sums compatible opponent reaches using a board-specific
rank/outcome table. The table is one `i8` per ordered hand pair (1,758,276 bytes),
shared across all terminals in the solve. Hand evaluation is not repeated during
iterations. Fold, win, loss and tie payoffs reuse the tree's exact accounting,
converted from twice-chip `i128` into chip `f64` at the solver boundary.

Regret values omit own initial range weight and the joint normalizer: they are
positive constants per information set and do not change regret matching.
Blockers still enter every terminal sum. Reported root EV **does** use own range
weights and the joint compatible-mass normalizer. Reaches are not normalized at
each node. Tests compare initial policy EV against independent scalar enumeration
through the tree payoff API, including blockers, raises and unequal stacks.

### Averaging and reporting

There is no hidden averaging default: callers choose `Averaging::Uniform` or
`Averaging::Linear` and `averaging_delay`. Complete iterations `t <= delay` are
excluded; later weights are 1 or `t-delay`, respectively. Strategy accumulation
uses **own** action reach, not opponent or joint reach. This is tested at a
downstream information set. Iteration numbering persists across calls, so
`solve(100); solve(100)` equals `solve(200)`.

`StrategyKind::Current` selects regret-matched policies; `Average` selects the
normalized strategy sums. A hand/information set with no average accumulation
falls back to uniform (including queries before the delay has elapsed). This
fallback is explicit and is not evidence of convergence.

`expected_values(kind)` returns both zero-sum root EVs in chips under the chosen
profile. `hand_report(id, hand, kind)` returns actions, their frequencies, and
conditional continuation EV per action. These include the original P/2 sunk-pot
baseline; they are not incremental profit from the queried node. An information
set with zero compatible opponent reach gets `None` action EVs, not fabricated
zero EV or NaNs. The action can be evaluated counterfactually even if the acting
player's own reach is zero. Unsupported input-range hands are rejected rather
than presenting an untrained policy as solved.

```rust
use poso::betting::{BettingConfig, StreetConfig};
use poso::cards::Board;
use poso::ranges::Range;
use poso::solver::{Averaging, RiverSolver, SolverOptions, StrategyKind};
use poso::tree::{Spot, State};
let board = "2c 3d 7h 9s Jc".split_whitespace()
    .map(str::parse).collect::<Result<Vec<_>, _>>()?;
let root = State::new(Spot {
    board: Board::new(&board)?, pot: 100, stacks: [100, 100], minimum_bet: 1,
})?;
// Check-only example keeps this documentation test quick.
let street = StreetConfig {
    bet_sizes: vec![], raise_sizes: vec![], include_all_in: false, max_raises: 0,
};
let config = BettingConfig { flop: street.clone(), turn: street.clone(), river: street };
let mut solver = RiverSolver::new(root, [Range::uniform(), Range::uniform()],
    &config, SolverOptions {
        averaging: Averaging::Linear, averaging_delay: 0,
        max_nodes: 100, max_depth: 20,
    })?;
solver.solve(1)?;
let ev = solver.expected_values(StrategyKind::Average);
assert!((ev[0] + ev[1]).abs() < 1e-7);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Run the milestone

```sh
cargo run --release --example river -- 2000
cargo test --test solver
cargo test
cargo test --release -- --ignored
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The example uses an actual Hold'em river `2c 3d 7h 9s Jc`, pot 100, stacks
100/100. OOP has `AsAh` and `4s4h` equally weighted; IP has `KsKh`. Both players
can check or bet all-in; no raises. OOP's value hand beats the bluff-catcher,
and the bluff hand loses to it. The analytic equilibrium has value bet 100%,
bluff bet 50%, IP call 50%; OOP EV is 25 chips under the documented convention.
Observed after 2,000 complete iterations, linear averaging with delay 10:

```text
value bet       1.000000
bluff bet       0.500454
IP call         0.499821
OOP average EV 25.000008
```

This is the backend smoke/demo executable; the local website is a separate interface. It reports EV snapshots, not exploitability. Analytic frequencies and
zero-sum EV are useful checks, but neither proves low exploitability in general.
Step 6 will supply a separate best-response calculation and convergence metric.

### Remaining limitations

This implementation is single-threaded and allocation-heavy, with an O(N²)
terminal kernel and no performance claims. Tree caps bound node count and depth,
not a precise byte budget. Depth must be 1–256; the root is depth 0. Each decision
stores two `f64` arrays of `actions × 1326` elements. Large abstractions may need
much smaller node budgets than simple examples. There is no early stopping,
checkpoint serialization, in-solver cancellation, or an automatic exploitability target. `f64` regret/EV arithmetic is approximate (chip amounts above 2^53
lose integer precision); extremely long runs and extreme input scales can exceed
useful numerical accuracy. Turn/flop chance traversal is still Step 7.
