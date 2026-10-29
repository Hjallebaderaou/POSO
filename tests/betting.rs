use poso::betting::{
    BettingConfig, BettingError, PotFraction, RaiseBasis, RaiseSize, StreetConfig,
};
use poso::cards::Board;
use poso::tree::{Action, Action::*, Node, Spot, State};
fn fraction(n: u32, d: u32) -> PotFraction {
    PotFraction::new(n, d).unwrap()
}
fn config(all_in: bool, cap: u64) -> BettingConfig {
    let sizes = vec![fraction(33, 100), fraction(75, 100), fraction(150, 100)];
    let street = StreetConfig {
        bet_sizes: sizes.clone(),
        raise_sizes: sizes
            .into_iter()
            .flat_map(|fraction| {
                [
                    RaiseSize {
                        fraction,
                        basis: RaiseBasis::PotAfterCall,
                    },
                    RaiseSize {
                        fraction,
                        basis: RaiseBasis::CurrentPot,
                    },
                ]
            })
            .collect(),
        include_all_in: all_in,
        max_raises: cap,
    };
    BettingConfig {
        flop: street.clone(),
        turn: street.clone(),
        river: street,
    }
}
fn spot(board: &str, pot: u64, stacks: [u64; 2], min: u64) -> State {
    let cards = board
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    State::new(Spot {
        board: Board::new(&cards).unwrap(),
        pot,
        stacks,
        minimum_bet: min,
    })
    .unwrap()
}
fn river(pot: u64, stacks: [u64; 2]) -> State {
    spot("2c 3d 7h 9s Jc", pot, stacks, 10)
}
#[test]
fn opening_sizes_and_both_raise_formulas() {
    let c = config(true, 2);
    let s = river(100, [1000, 1000]);
    assert_eq!(
        c.actions(&s).unwrap(),
        [Check, BetTo(33), BetTo(75), BetTo(150), BetTo(1000)]
    );
    let s = s.act(BetTo(50)).unwrap();
    // Pot=150, call=50: 75% current gives 50+113=163;
    // 75% after call gives 50+150=200.
    assert_eq!(
        c.actions(&s).unwrap(),
        [
            Fold,
            Call,
            RaiseTo(100),
            RaiseTo(116),
            RaiseTo(163),
            RaiseTo(200),
            RaiseTo(275),
            RaiseTo(350),
            RaiseTo(1000)
        ]
    );
}
#[test]
fn exact_fractions_round_half_up() {
    assert!(PotFraction::new(1, 0).is_err());
    assert!(PotFraction::new(0, 1).is_err());
    let f = fraction(1, 3);
    assert_eq!((f.numerator(), f.denominator()), (1, 3));
    let mut c = config(false, 0);
    c.river.bet_sizes = vec![fraction(1, 2), fraction(1, 3), fraction(33, 100)];
    let s = spot("2c 3d 7h 9s Jc", 101, [1000, 1000], 1);
    assert_eq!(
        c.actions(&s).unwrap(),
        [Check, BetTo(33), BetTo(34), BetTo(51)]
    );
}
#[test]
fn undersized_omitted_but_short_all_in_is_legal() {
    let mut c = config(false, 2);
    c.river.bet_sizes = vec![fraction(1, 100)];
    assert_eq!(c.actions(&river(100, [100, 100])).unwrap(), [Check]);
    c.river.include_all_in = true;
    assert_eq!(c.actions(&river(100, [5, 100])).unwrap(), [Check, BetTo(5)]);
    let s = river(100, [100, 45]).act(BetTo(30)).unwrap();
    c.river.raise_sizes.clear();
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call, RaiseTo(45)]);
    let s = c.act(&s, RaiseTo(45)).unwrap();
    assert_eq!(s.raises_this_street(), 1);
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call]);
}
#[test]
fn oversize_caps_and_duplicates_merge() {
    let mut c = config(true, 2);
    c.river
        .bet_sizes
        .extend([fraction(300, 100), fraction(33, 100)]);
    assert_eq!(
        c.actions(&river(100, [50, 100])).unwrap(),
        [Check, BetTo(33), BetTo(50)]
    );
    c.river.include_all_in = false;
    assert_eq!(
        c.actions(&river(100, [20, 100])).unwrap(),
        [Check, BetTo(20)]
    );
    let s = river(100, [200, 20]).act(BetTo(100)).unwrap();
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call]); // A short call isn't a raise.
    let t = c.act(&s, Call).unwrap();
    assert_eq!(t.pot(), 140);
    assert_eq!(t.stacks(), [180, 0]);
}
#[test]
fn strict_caps_zero_and_one_and_membership_enforcement() {
    let mut c = config(true, 0);
    let root = river(100, [1000, 1000]);
    assert!(c.actions(&root).unwrap().contains(&BetTo(1000)));
    let s = c.act(&root, BetTo(75)).unwrap();
    assert_eq!(s.raises_this_street(), 0);
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call]);
    assert_eq!(
        c.act(&s, RaiseTo(1000)).unwrap_err(),
        BettingError::ActionNotConfigured
    );
    c.river.max_raises = 1;
    let s = c.act(&s, RaiseTo(158)).unwrap(); // 75 + round(33% * 250).
    assert_eq!(s.raises_this_street(), 1);
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call]);
    assert!(c.act(&s, RaiseTo(1000)).is_err());
    assert!(c.act(&root, BetTo(50)).is_err()); // Poker legal, not configured.
}
#[test]
fn street_specific_configuration_and_counter_reset() {
    let mut c = config(false, 1);
    c.flop.bet_sizes = vec![fraction(1, 2)];
    c.flop.raise_sizes = vec![RaiseSize {
        fraction: fraction(1, 1),
        basis: RaiseBasis::PotAfterCall,
    }];
    c.turn.bet_sizes = vec![fraction(1, 4)];
    c.turn.max_raises = 0;
    c.river.bet_sizes = vec![fraction(1, 1)];
    let s = spot("2c 3d 7h", 100, [1000, 1000], 10);
    assert_eq!(c.actions(&s).unwrap(), [Check, BetTo(50)]);
    let s = c.act(&s, BetTo(50)).unwrap();
    let s = c.act(&s, RaiseTo(250)).unwrap();
    assert_eq!(s.raises_this_street(), 1);
    let s = c.act(&s, Call).unwrap();
    assert_eq!(s.raises_this_street(), 0);
    assert_eq!(s.node(), Node::Chance);
    assert!(c.actions(&s).is_err());
    let s = s.deal("9s".parse().unwrap()).unwrap();
    assert_eq!(c.actions(&s).unwrap(), [Check, BetTo(150)]);
    let s = c.act(&s, BetTo(150)).unwrap();
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call]);
    let s = c
        .act(&s, Call)
        .unwrap()
        .deal("Jc".parse().unwrap())
        .unwrap();
    assert_eq!(c.actions(&s).unwrap(), [Check, BetTo(600)]); // 900-pot bet capped to stack.
}
#[test]
fn zero_pot_empty_configs_and_all_in_opponent() {
    let mut c = config(false, 0);
    assert_eq!(c.actions(&river(0, [100, 100])).unwrap(), [Check]);
    c.river.bet_sizes.clear();
    c.river.raise_sizes.clear();
    let s = river(100, [100, 100]);
    assert_eq!(c.actions(&s).unwrap(), [Check]);
    let terminal = c.act(&c.act(&s, Check).unwrap(), Check).unwrap();
    assert!(c.actions(&terminal).is_err());
    assert!(c.act(&terminal, Check).is_err());
    c.river.include_all_in = true;
    let s = river(100, [100, 5])
        .act(Check)
        .unwrap()
        .act(BetTo(5))
        .unwrap();
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call]);
}
#[test]
fn huge_amounts_do_not_overflow() {
    let mut c = config(true, 2);
    c.river.bet_sizes = vec![fraction(u32::MAX, 1)];
    c.river.raise_sizes = vec![RaiseSize {
        fraction: fraction(u32::MAX, 1),
        basis: RaiseBasis::PotAfterCall,
    }];
    let s = river(u64::MAX - 200, [100, 100]);
    assert_eq!(c.actions(&s).unwrap(), [Check, BetTo(100)]);
    let s = s.act(BetTo(10)).unwrap();
    assert_eq!(c.actions(&s).unwrap(), [Fold, Call, RaiseTo(100)]);
}
#[test]
fn enumerated_abstract_tree_has_only_legal_unique_actions() {
    fn visit(c: &BettingConfig, s: State, depth: usize, leaves: &mut usize) {
        assert!(depth < 20);
        assert_eq!(s.pot() + s.stacks().iter().sum::<u64>(), 180);
        if let Node::Terminal(_) = s.node() {
            *leaves += 1;
            return;
        }
        let actions = c.actions(&s).unwrap();
        assert!(!actions.is_empty());
        for (i, &a) in actions.iter().enumerate() {
            assert!(!actions[..i].contains(&a));
            assert!(s.act(a).is_ok());
            if matches!(a, Action::RaiseTo(_)) {
                assert!(s.raises_this_street() < 1);
            }
            visit(c, c.act(&s, a).unwrap(), depth + 1, leaves);
        }
    }
    let mut leaves = 0;
    visit(&config(true, 1), river(100, [40, 40]), 0, &mut leaves);
    assert!(leaves > 10);
}
