use poso::{
    cards::{Card, CardError},
    evaluator::{evaluate, Category},
};

fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace().map(|s| s.parse().unwrap()).collect()
}
fn key(c: &[Card]) -> (u8, [u8; 5]) {
    let v = evaluate(c).unwrap();
    (v.category() as u8, v.tiebreakers())
}

// Independent five-card oracle: sort rank groups and detect consecutive ranks.
// No production bitmask helpers or category-building functions are reused.
fn reference(c: &[Card]) -> (u8, [u8; 5]) {
    assert_eq!(c.len(), 5);
    let mut ranks: Vec<u8> = c.iter().map(|c| c.rank()).collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    let flush = c.iter().all(|x| x.suit() == c[0].suit());
    let wheel = ranks == [12, 3, 2, 1, 0];
    let run = ranks.windows(2).all(|p| p[0] == p[1] + 1);
    let high = if wheel { 3 } else { ranks[0] };
    let mut groups: Vec<(usize, u8)> = Vec::new();
    for r in 0..13 {
        let n = ranks.iter().filter(|&&v| v == r).count();
        if n > 0 {
            groups.push((n, r));
        }
    }
    groups.sort_unstable_by(|a, b| b.cmp(a));
    let (category, kickers) = if flush && (wheel || run) {
        (8, vec![high])
    } else if groups[0].0 == 4 {
        (7, groups.iter().map(|g| g.1).collect())
    } else if groups[0].0 == 3 && groups[1].0 == 2 {
        (6, groups.iter().map(|g| g.1).collect())
    } else if flush {
        (5, ranks)
    } else if wheel || run {
        (4, vec![high])
    } else {
        let cat = match (groups[0].0, groups[1].0) {
            (3, _) => 3,
            (2, 2) => 2,
            (2, _) => 1,
            _ => 0,
        };
        (cat, groups.iter().map(|g| g.1).collect())
    };
    let mut result = [0; 5];
    result[..kickers.len()].copy_from_slice(&kickers);
    (category, result)
}
fn reference_best(c: &[Card]) -> (u8, [u8; 5]) {
    let mut best = (0, [0; 5]);
    for a in 0..c.len() - 4 {
        for b in a + 1..c.len() - 3 {
            for d in b + 1..c.len() - 2 {
                for e in d + 1..c.len() - 1 {
                    for f in e + 1..c.len() {
                        best = best.max(reference(&[c[a], c[b], c[d], c[e], c[f]]));
                    }
                }
            }
        }
    }
    best
}

#[test]
fn categories_and_ordering() {
    let cases = [
        ("As Kd 9h 7c 4s 3d 2c", Category::HighCard),
        ("As Ad Kh 9c 7s 4d 2c", Category::OnePair),
        ("As Ad Kh Kc 7s 4d 2c", Category::TwoPair),
        ("As Ad Ah Kc 7s 4d 2c", Category::ThreeOfAKind),
        ("As 2d 3h 4c 5s Kd Qc", Category::Straight),
        ("As Js 9s 6s 3s Kd Qc", Category::Flush),
        ("As Ad Ah Kc Ks Kd 2c", Category::FullHouse),
        ("As Ad Ah Ac Ks Kd 2c", Category::FourOfAKind),
        ("As Ks Qs Js Ts 2d 3c", Category::StraightFlush),
    ];
    let mut previous = None;
    for (text, category) in cases {
        let c = cards(text);
        let v = evaluate(&c).unwrap();
        assert_eq!(v.category(), category);
        assert_eq!(key(&c), reference_best(&c));
        if let Some(p) = previous {
            assert!(v > p);
        }
        previous = Some(v);
    }
}
#[test]
fn kickers_and_holdem_edge_cases() {
    for (better, worse) in [
        ("6c 5d 4h 3s 2c", "Ac 2d 3h 4s 5c"),
        ("Ac Ad Ks Qh Jd", "Ac Ad Ks Qh Td"),
        ("Ac Ad Kc Kd Qh", "Ac Ad Qc Qd Kh"),
        ("Ac Ad Ah Ks Qd", "Ac Ad Ah Ks Jd"),
        ("Ac Jc 9c 6c 4c", "Ad Jd 9d 6d 3d"),
        ("Ac Ad Ah Ks Kd", "Kc Kd Kh As Ad"),
        ("Ac Ad Ah As Kd", "Ac Ad Ah As Qd"),
        ("6c 5c 4c 3c 2c", "Ac 2c 3c 4c 5c"),
    ] {
        assert!(evaluate(&cards(better)).unwrap() > evaluate(&cards(worse)).unwrap());
    }
    // Third pair supplies the kicker; two trips supply a full house.
    for text in [
        "As Ad Ks Kd Qs Qd 2c",
        "As Ad Ah Ks Kd Kh 2c",
        "As 2s 3s 4s 5s Kd Qc",
        "As Ks Qs Js 9s 8s 2s",
    ] {
        let c = cards(text);
        assert_eq!(key(&c), reference_best(&c));
    }
    assert_eq!(
        evaluate(&cards("As Ks Qs Js Ts 2d 3c")),
        evaluate(&cards("As Ks Qs Js Ts 8d 9c"))
    ); // Play the board.
    assert_eq!(
        evaluate(&cards("As As Kd Qc Jh")),
        Err(CardError::DuplicateCard)
    );
    for n in [0, 1, 2, 3, 4, 8] {
        let c: Vec<_> = (0..n).map(|i| Card::from_id(i).unwrap()).collect();
        assert_eq!(evaluate(&c), Err(CardError::InvalidCardCount));
    }
}
#[test]
fn deterministic_differential_and_invariance() {
    let mut seed = 0x123456789abcdefu64;
    for sample in 0..10_000 {
        let mut deck: Vec<_> = (0..52).map(|i| Card::from_id(i).unwrap()).collect();
        let n = 5 + sample % 3;
        for i in 0..n {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let j = i + seed as usize % (52 - i);
            deck.swap(i, j);
        }
        let mut c = deck[..n].to_vec();
        let expected = key(&c);
        assert_eq!(expected, reference_best(&c), "{c:?}");
        c.reverse();
        assert_eq!(key(&c), expected);
        // Every global permutation of four suit labels preserves strength.
        for a in 0..4 {
            for b in 0..4 {
                for d in 0..4 {
                    for e in 0..4 {
                        let p = [a, b, d, e];
                        if a == b || a == d || a == e || b == d || b == e || d == e {
                            continue;
                        }
                        let mapped: Vec<_> = c
                            .iter()
                            .map(|x| Card::from_id(x.rank() * 4 + p[x.suit() as usize]).unwrap())
                            .collect();
                        assert_eq!(key(&mapped), expected);
                    }
                }
            }
        }
    }
}
#[test]
#[ignore = "exhaustive 2,598,960-hand oracle check; run in release mode"]
fn exhaustive_five_card_distribution() {
    let deck: Vec<_> = (0..52).map(|i| Card::from_id(i).unwrap()).collect();
    let mut counts = [0u64; 9];
    let mut strengths = std::collections::HashSet::new();
    for a in 0..48 {
        for b in a + 1..49 {
            for c in b + 1..50 {
                for d in c + 1..51 {
                    for e in d + 1..52 {
                        let hand = [deck[a], deck[b], deck[c], deck[d], deck[e]];
                        let k = key(&hand);
                        assert_eq!(k, reference(&hand));
                        counts[k.0 as usize] += 1;
                        strengths.insert(k);
                    }
                }
            }
        }
    }
    assert_eq!(
        counts,
        [1302540, 1098240, 123552, 54912, 10200, 5108, 3744, 624, 40]
    );
    assert_eq!(strengths.len(), 7462);
}
