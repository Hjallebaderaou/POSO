use poso::cards::{Board, Card, HoleCards};
use poso::ranges::{combos, compatible, ComboIndex, JointRanges, Range, RangeError, COMBO_COUNT};
fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace().map(|s| s.parse().unwrap()).collect()
}
fn hole(s: &str) -> HoleCards {
    let c = cards(s);
    HoleCards::new(c[0], c[1]).unwrap()
}
fn board(s: &str) -> Board {
    Board::new(&cards(s)).unwrap()
}
fn range(entries: &[(&str, f64)]) -> Range {
    let mut r = Range::empty();
    for &(h, w) in entries {
        r.set(hole(h), w).unwrap();
    }
    r
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}
#[test]
fn exhaustive_combo_bijection() {
    assert_eq!(combos().len(), COMBO_COUNT);
    let mut masks = std::collections::HashSet::new();
    for (i, &h) in combos().iter().enumerate() {
        assert_eq!(ComboIndex::of(h).get(), i);
        assert_eq!(ComboIndex::new(i).unwrap().hand(), h);
        let [a, b] = h.cards();
        assert!(a.id() < b.id());
        assert_eq!(ComboIndex::of(HoleCards::new(b, a).unwrap()).get(), i);
        assert!(masks.insert(h.mask()));
    }
    assert!(ComboIndex::new(COMBO_COUNT).is_err());
    assert!(ComboIndex::new(usize::MAX).is_err());
    assert_eq!(combos()[0], hole("2c 2d"));
    assert_eq!(combos()[1325], hole("Ah As"));
}
#[test]
fn rank_class_counts() {
    let pairs = combos()
        .iter()
        .filter(|h| h.cards()[0].rank() == h.cards()[1].rank())
        .count();
    let suited = combos()
        .iter()
        .filter(|h| h.cards()[0].suit() == h.cards()[1].suit())
        .count();
    assert_eq!(pairs, 78);
    assert_eq!(suited, 312);
    assert_eq!(COMBO_COUNT - pairs - suited, 936);
    for rank in 0..13 {
        assert_eq!(
            combos()
                .iter()
                .filter(|h| h.cards().iter().all(|c| c.rank() == rank))
                .count(),
            6
        );
    }
}
#[test]
fn weights_validation_and_large_normalization() {
    let h = hole("As Ah");
    let mut r = Range::empty();
    for w in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(r.set(h, w), Err(RangeError::InvalidWeight));
        let mut v = [0.0; COMBO_COUNT];
        v[0] = w;
        assert_eq!(
            Range::from_weights(v).unwrap_err(),
            RangeError::InvalidWeight
        );
    }
    assert_eq!(r.normalized().unwrap_err(), RangeError::EmptyRange);
    r.set(h, 2.0).unwrap();
    assert_eq!(r.weight(h), 2.0);
    assert_eq!(r.weight(hole("Ah As")), 2.0);
    assert_eq!(r.active_count(), 1);
    assert_eq!(r.normalized().unwrap().weight(h), 1.0);
    assert_eq!(r.weight(h), 2.0); // no mutation
    let big = Range::from_weights([f64::MAX; COMBO_COUNT])
        .unwrap()
        .normalized()
        .unwrap();
    close(big.weights().iter().sum(), 1.0);
    close(big.weight(h), 1.0 / COMBO_COUNT as f64);
}
#[test]
fn blocker_counts_on_every_street_and_idempotence() {
    let r = Range::uniform();
    for (s, n) in [
        ("", 1326),
        ("As Kd Qh", 1176),
        ("As Kd Qh Jc", 1128),
        ("As Kd Qh Jc Ts", 1081),
    ] {
        let b = board(s);
        let filtered = r.without_cards(b.cards()).unwrap();
        assert_eq!(filtered.active_count(), n);
        assert_eq!(filtered.without_cards(b.cards()).unwrap(), filtered);
        for (&h, &w) in combos().iter().zip(filtered.weights()) {
            assert_eq!(w, if h.mask() & b.mask() == 0 { 1.0 } else { 0.0 });
        }
    }
    assert_eq!(r.active_count(), 1326);
    assert!(r.without_cards(&cards("As As")).is_err());
    assert_eq!(r.without_cards(&cards("As")).unwrap().active_count(), 1275);
}
#[test]
fn exact_hero_conditioning() {
    let b = board("2c 3d 7h 9s Jc");
    let r = Range::uniform().conditioned_on(&b, hole("As Ah")).unwrap();
    assert_eq!(r.active_count(), 990); // choose(45,2)
    close(r.weights().iter().sum(), 1.0);
    assert_eq!(r.weight(hole("As Ks")), 0.0);
    close(r.weight(hole("Ks Kh")), 1.0 / 990.0);
    assert!(Range::uniform().conditioned_on(&b, hole("2c As")).is_err());
    assert_eq!(
        range(&[("As Ks", 1.0)])
            .conditioned_on(&b, hole("As Ah"))
            .unwrap_err(),
        RangeError::EmptyRange
    );
}
#[test]
fn weighted_joint_distribution_and_conditional_marginals() {
    let oop = range(&[("As Ah", 2.0), ("Ks Kh", 1.0)]);
    let ip = range(&[("As Qs", 3.0), ("Qc Qd", 1.0)]);
    // Products: AA/AQ invalid; AA/QQ=2, KK/AQ=3, KK/QQ=1. Z=6.
    let j = JointRanges::new(&board("2c 3d 7h 9s Jc"), &oop, &ip).unwrap();
    close(j.compatible_mass(), 0.5);
    assert_eq!(j.probability(hole("As Ah"), hole("As Qs")), 0.0);
    close(j.probability(hole("As Ah"), hole("Qc Qd")), 2.0 / 6.0);
    close(j.probability(hole("Ks Kh"), hole("As Qs")), 3.0 / 6.0);
    close(j.probability(hole("Ks Kh"), hole("Qc Qd")), 1.0 / 6.0);
    assert_eq!(j.probability(hole("2c Ah"), hole("Qc Qd")), 0.0);
    let m = j.marginals();
    close(m[0].weight(hole("As Ah")), 1.0 / 3.0);
    close(m[0].weight(hole("Ks Kh")), 2.0 / 3.0);
    close(m[1].weight(hole("As Qs")), 0.5);
    close(m[1].weight(hole("Qc Qd")), 0.5);
    close(j.independent()[0].weight(hole("As Ah")), 2.0 / 3.0);
    assert!(m[0].weight(hole("As Ah")) > 0.0); // Not globally removed by possible As.
    assert_eq!(oop.weight(hole("As Ah")), 2.0);
}
#[test]
fn zero_support_and_incompatible_ranges_report_errors() {
    let b = board("As Kd Qh Jc Ts");
    assert_eq!(
        JointRanges::new(&b, &Range::empty(), &Range::uniform()).unwrap_err(),
        RangeError::EmptyRange
    );
    assert_eq!(
        JointRanges::new(&b, &range(&[("As Ah", 1.0)]), &Range::uniform()).unwrap_err(),
        RangeError::EmptyRange
    );
    let b = board("");
    assert_eq!(
        JointRanges::new(&b, &range(&[("As Ah", 1.0)]), &range(&[("As Ks", 1.0)])).unwrap_err(),
        RangeError::NoCompatibleHands
    );
    assert!(!compatible(hole("As Ah"), hole("As Ks")));
    assert!(compatible(hole("As Ah"), hole("Ks Kh")));
}
#[test]
fn full_range_joint_matches_uniform_dealing() {
    let b = board("2c 3d 7h 9s Jc");
    let j = JointRanges::new(&b, &Range::uniform(), &Range::uniform()).unwrap();
    close(j.compatible_mass(), 990.0 / 1081.0);
    let m = j.marginals();
    let mut sum = 0.0;
    let mut pair_count = 0;
    for (i, &a) in combos().iter().enumerate() {
        let mut row = 0.0;
        for &other in combos() {
            let p = j.probability(a, other);
            assert!(p.is_finite() && p >= 0.0);
            if p > 0.0 {
                pair_count += 1;
                close(p, 1.0 / (1081.0 * 990.0));
            }
            row += p;
        }
        close(row, m[0].weights()[i]);
        sum += row;
    }
    assert_eq!(pair_count, 1081 * 990);
    close(sum, 1.0);
    for marginal in &m {
        close(marginal.weights().iter().sum(), 1.0);
    }
}
#[test]
fn joint_scale_and_player_swap_invariance() {
    let b = board("2c 3d 7h");
    let a = range(&[("As Ah", 1.0), ("Ks Kh", 2.0), ("2c 2d", 8.0)]);
    let scaled = range(&[("As Ah", 10.0), ("Ks Kh", 20.0), ("2c 2d", 80.0)]);
    let other = range(&[("As Qs", 3.0), ("Qc Qd", 7.0)]);
    let j = JointRanges::new(&b, &a, &other).unwrap();
    let s = JointRanges::new(&b, &scaled, &other).unwrap();
    let swap = JointRanges::new(&b, &other, &a).unwrap();
    for &h in combos() {
        for k in [hole("As Qs"), hole("Qc Qd")] {
            close(j.probability(h, k), s.probability(h, k));
            close(j.probability(h, k), swap.probability(k, h));
        }
    }
}

#[test]
fn numerical_underflow_is_not_reported_as_impossible_hands() {
    // Only the pair of rare hands is compatible, and 1e-200 * 1e-200
    // cannot be represented in f64. Fail rather than emit NaN probabilities.
    let a = range(&[("As Ah", 1.0), ("Ks Kh", 1e-200)]);
    let b = range(&[("As Ks", 1.0), ("Ah Qh", 1e-200)]);
    assert_eq!(
        JointRanges::new(&board(""), &a, &b).unwrap_err(),
        RangeError::NumericalUnderflow
    );
}

#[test]
fn weighted_joint_matches_independent_raw_product_oracle() {
    let b = board("2c 3d 7h 9s Jc");
    let mut a = Range::empty();
    let mut other = Range::empty();
    for (i, &h) in combos().iter().enumerate() {
        if i % 17 == 0 {
            a.set(h, (i % 7 + 1) as f64 / 7.0).unwrap();
        }
        if i % 23 == 0 {
            other.set(h, (i % 11 + 1) as f64 / 11.0).unwrap();
        }
    }
    // Reference sums raw products directly, before independent normalization.
    let mut products = Vec::new();
    let mut total = 0.0;
    for &h in combos() {
        for &k in combos() {
            if (h.mask() | k.mask()) & b.mask() == 0 && compatible(h, k) {
                let raw = a.weight(h) * other.weight(k);
                if raw > 0.0 {
                    products.push((h, k, raw));
                    total += raw;
                }
            }
        }
    }
    let joint = JointRanges::new(&b, &a, &other).unwrap();
    let swapped = JointRanges::new(&b, &other, &a).unwrap();
    let mut sum = 0.0;
    for (h, k, w) in products {
        let p = joint.probability(h, k);
        close(p, w / total);
        close(p, swapped.probability(k, h));
        sum += p;
    }
    close(sum, 1.0);
}
