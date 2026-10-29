use poso::cards::{Board, Card, HoleCards};
use poso::tree::{Action::*, Node, Player, Spot, State, Street, Terminal, TreeError};
fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace().map(|x| x.parse().unwrap()).collect()
}
fn hole(s: &str) -> HoleCards {
    let c = cards(s);
    HoleCards::new(c[0], c[1]).unwrap()
}
fn spot(board: &str, pot: u64, stacks: [u64; 2]) -> State {
    State::new(Spot {
        board: Board::new(&cards(board)).unwrap(),
        pot,
        stacks,
        minimum_bet: 10,
    })
    .unwrap()
}
fn river(stacks: [u64; 2]) -> State {
    spot("2c 3d 7h 9s Jc", 100, stacks)
}
fn check_down(mut s: State) -> State {
    loop {
        s = match s.node() {
            Node::Player(_) => s.act(Check).unwrap(),
            Node::Chance => {
                let c = s.possible_cards().unwrap()[0];
                s.deal(c).unwrap()
            }
            Node::Terminal(_) => return s,
        }
    }
}
#[test]
fn config_and_no_private_cards_in_public_states() {
    assert!(State::new(Spot {
        board: Board::new(&[]).unwrap(),
        pot: u64::MAX,
        stacks: [1, 1],
        minimum_bet: 1
    })
    .is_err());
    assert!(State::new(Spot {
        board: Board::new(&[]).unwrap(),
        pot: 1,
        stacks: [1, 1],
        minimum_bet: 0
    })
    .is_err());
    let s = river([100, 100]);
    assert_eq!(s.node(), Node::Player(Player::Oop));
    let own = hole("As Ah");
    assert_eq!(
        s.information_key(7, own).unwrap(),
        s.information_key(7, hole("Ah As")).unwrap()
    );
    assert_ne!(
        s.information_key(7, own).unwrap(),
        s.information_key(8, own).unwrap()
    );
    assert_ne!(
        s.information_key(7, own).unwrap(),
        s.information_key(7, hole("Ks Kh")).unwrap()
    );
    assert!(s.information_key(7, hole("2c As")).is_err());
}
#[test]
fn check_check_and_street_resets() {
    let s = spot("2c 3d 7h", 100, [200, 200]);
    assert_eq!(s.street(), Some(Street::Flop));
    let s = s.act(BetTo(30)).unwrap().act(Call).unwrap();
    assert_eq!(s.node(), Node::Chance);
    assert_eq!(s.committed(), [0, 0]);
    assert_eq!(s.pot(), 160);
    let s = s.deal("9s".parse().unwrap()).unwrap();
    assert_eq!(s.street(), Some(Street::Turn));
    assert_eq!(s.node(), Node::Player(Player::Oop));
    let s = s.act(Check).unwrap().act(Check).unwrap();
    let s = s.deal("Jc".parse().unwrap()).unwrap();
    assert_eq!(s.street(), Some(Street::River));
    assert_eq!(check_down(s).node(), Node::Terminal(Terminal::Showdown));
}
#[test]
fn legality_and_full_raise_sizes() {
    let root = river([200, 200]);
    for a in [Call, Fold, RaiseTo(10), BetTo(0), BetTo(9), BetTo(201)] {
        assert!(root.act(a).is_err(), "{a:?}");
    }
    let s = root.act(BetTo(30)).unwrap();
    assert_eq!(s.to_call().unwrap(), 30);
    for a in [Check, BetTo(60), RaiseTo(59), RaiseTo(30)] {
        assert!(s.act(a).is_err());
    }
    let s = s.act(RaiseTo(60)).unwrap();
    assert!(s.act(RaiseTo(89)).is_err());
    let s = s.act(RaiseTo(90)).unwrap().act(Call).unwrap();
    assert_eq!(s.pot(), 280);
    assert_eq!(s.stacks(), [110, 110]);
    assert_eq!(s.node(), Node::Terminal(Terminal::Showdown));
    assert_eq!(root.pot(), 100); // Persistent lazy branches don't mutate their parent.
}
#[test]
fn short_all_in_and_no_reopening() {
    let s = river([100, 45])
        .act(BetTo(30))
        .unwrap()
        .act(RaiseTo(45))
        .unwrap();
    assert_eq!(s.to_call().unwrap(), 15);
    assert!(s.act(RaiseTo(75)).is_err());
    let s = s.act(Call).unwrap();
    assert_eq!(s.pot(), 190);
    assert_eq!(s.stacks(), [55, 0]);
    assert_eq!(s.node(), Node::Terminal(Terminal::Showdown));
    // A player who checked can still call a short opening all-in.
    let s = river([100, 5]).act(Check).unwrap().act(BetTo(5)).unwrap();
    assert!(s.act(RaiseTo(15)).is_err()); // No opponent chips remain to contest.
    assert_eq!(s.act(Call).unwrap().pot(), 110);
}
#[test]
fn refunds_fold_and_exact_odd_pot_accounting() {
    let s = river([200, 20]).act(BetTo(100)).unwrap().act(Call).unwrap();
    assert_eq!(s.pot(), 140);
    assert_eq!(s.stacks(), [180, 0]);
    let win = s
        .showdown_payoffs_twice([hole("As Ah"), hole("Ks Kh")])
        .unwrap();
    assert_eq!(win, [140, -140]);
    let s = river([200, 200])
        .act(BetTo(30))
        .unwrap()
        .act(RaiseTo(60))
        .unwrap()
        .act(Fold)
        .unwrap();
    assert_eq!(s.pot(), 160); // Uncalled 30 refunded.
    assert_eq!(s.stacks(), [170, 170]);
    assert_eq!(s.fold_payoffs_twice().unwrap(), [-160, 160]);
    let s = spot("2c 3d 7h 9s Jc", 101, [10, 10])
        .act(BetTo(10))
        .unwrap()
        .act(Fold)
        .unwrap();
    assert_eq!(s.fold_payoffs_twice().unwrap(), [101, -101]);
}
#[test]
fn showdown_ties_and_blockers() {
    let s = check_down(river([100, 100]));
    assert_eq!(
        s.showdown_payoffs_twice([hole("As Ah"), hole("Ks Kh")])
            .unwrap(),
        [100, -100]
    );
    assert_eq!(
        s.showdown_payoffs_twice([hole("Ks Kh"), hole("As Ah")])
            .unwrap(),
        [-100, 100]
    );
    assert!(s
        .showdown_payoffs_twice([hole("As Ah"), hole("As Kh")])
        .is_err());
    assert!(s
        .showdown_payoffs_twice([hole("2c Ah"), hole("As Kh")])
        .is_err());
    let s = check_down(spot("As Ks Qs Js Ts", 101, [100, 100]));
    assert_eq!(
        s.showdown_payoffs_twice([hole("2c 3c"), hole("4c 5c")])
            .unwrap(),
        [0, 0]
    );
}
#[test]
fn chance_is_conditional_and_all_ins_run_out_without_actions() {
    let s = spot("2c 3d 7h", 100, [0, 200]);
    assert_eq!(s.node(), Node::Chance);
    assert_eq!(s.possible_cards().unwrap().len(), 49);
    let hands = [hole("As Ah"), hole("Ks Kh")];
    let deals = s.conditional_deals(hands).unwrap();
    assert_eq!(deals.len(), 45);
    assert!((deals.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() < 1e-12);
    assert!(deals
        .iter()
        .all(|(c, _)| c.mask() & (hands[0].mask() | hands[1].mask()) == 0));
    assert!(s.conditional_deals([hole("2c Ah"), hole("Ks Kh")]).is_err());
    assert!(s.deal("2c".parse().unwrap()).is_err());
    let t = s.deal("4c".parse().unwrap()).unwrap();
    assert_eq!(t.node(), Node::Chance);
    assert_eq!(t.conditional_deals(hands).unwrap().len(), 44);
    let r = t.deal("5c".parse().unwrap()).unwrap();
    assert_eq!(r.node(), Node::Terminal(Terminal::Showdown));
    assert_eq!(r.act(Check).unwrap_err(), TreeError::WrongNode);
    assert!(r.deal("6c".parse().unwrap()).is_err());
    assert!(r.possible_cards().is_err());
    assert!(r.information_key(1, hands[0]).is_err());
}
#[test]
fn flop_chance_three_cards_and_fold_node_guards() {
    let s = spot("", 100, [100, 100]);
    assert_eq!(s.possible_cards().unwrap().len(), 52);
    let s = s.deal("As".parse().unwrap()).unwrap();
    assert_eq!(s.node(), Node::Chance);
    assert_eq!(s.possible_cards().unwrap().len(), 51);
    let s = s
        .deal("Kd".parse().unwrap())
        .unwrap()
        .deal("2c".parse().unwrap())
        .unwrap();
    assert_eq!(s.node(), Node::Player(Player::Oop));
    assert!(s.fold_payoffs_twice().is_err());
    assert!(s
        .showdown_payoffs_twice([hole("3c 4c"), hole("5c 6c")])
        .is_err());
    assert!(s.conditional_deals([hole("3c 4c"), hole("5c 6c")]).is_err());
    let s = s
        .act(Check)
        .unwrap()
        .act(BetTo(10))
        .unwrap()
        .act(Fold)
        .unwrap();
    assert_eq!(s.fold_payoffs_twice().unwrap(), [-100, 100]);
    assert!(s.act(Check).is_err());
}

// Enumerate a small REAL Hold'em river action tree, not a toy poker variant.
// Every terminal must conserve chips and have zero-sum utility.
#[test]
fn enumerate_small_river_tree_invariants() {
    fn visit(s: State, depth: usize, leaves: &mut usize) {
        assert!(depth < 20);
        assert_eq!(s.pot() + s.stacks().iter().sum::<u64>(), 140);
        if let Node::Terminal(t) = s.node() {
            let u = match t {
                Terminal::Fold { .. } => s.fold_payoffs_twice().unwrap(),
                Terminal::Showdown => s
                    .showdown_payoffs_twice([hole("As Ah"), hole("Ks Kh")])
                    .unwrap(),
            };
            assert_eq!(u[0] + u[1], 0);
            *leaves += 1;
            return;
        }
        assert!(matches!(s.node(), Node::Player(_)));
        for a in [Check, Fold, Call, BetTo(10), BetTo(20), RaiseTo(20)] {
            if let Ok(child) = s.act(a) {
                visit(child, depth + 1, leaves);
            }
        }
    }
    let mut leaves = 0;
    visit(river([20, 20]), 0, &mut leaves);
    assert!(leaves > 10);
}
