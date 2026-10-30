//! Calibrated switches for judged decisions (`docs/05-functors.md` §7,
//! `docs/12-calibrated-switches.md`): a calibration map from a judge's
//! probability to one that comes true as often as it says, a threshold
//! derived from the developer's costs, and the scores that tell whether
//! a map is any good. Pure functions; no I/O.

/// A Platt map, `p' = σ(a · logit p + b)`. Two parameters, because labels
/// are few; [`Platt::identity`] leaves p as the judge gave it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Platt {
    pub a: f64,
    pub b: f64,
}

/// The labelled decisions a map is fitted on: the judge's p for the
/// target it chose, and whether that target was right.
pub type Labelled = (f64, bool);

fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-6, 1.0 - 1e-6);
    (p / (1.0 - p)).ln()
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

impl Platt {
    pub fn identity() -> Self {
        Self { a: 1.0, b: 0.0 }
    }

    pub fn apply(&self, p: f64) -> f64 {
        sigmoid(self.a * logit(p) + self.b)
    }

    /// Logistic regression of the outcome on `logit p` (damped Newton:
    /// a step is halved until the objective falls, since a plain Newton
    /// step can overshoot and oscillate when the labels sit at a few p),
    /// with a Gaussian prior of weight `pull` centred on the identity, so
    /// that a handful of labels moves the map only as far as they warrant.
    pub fn fit(points: &[Labelled], pull: f64) -> Self {
        let objective = |w: &Self| -> f64 {
            let prior = 0.5 * pull * ((w.a - 1.0).powi(2) + w.b.powi(2));
            points.iter().fold(prior, |acc, &(p, y)| {
                let z = w.a * logit(p) + w.b;
                // −log σ(z) = log(1 + e^−z), written to stay finite.
                let softplus = |t: f64| t.max(0.0) + (-t.abs()).exp().ln_1p();
                acc + if y { softplus(-z) } else { softplus(z) }
            })
        };
        let mut w = Self::identity();
        for _ in 0..100 {
            // Gradient and Hessian of the negative log posterior.
            let (mut ga, mut gb) = (pull * (w.a - 1.0), pull * w.b);
            let (mut haa, mut hab, mut hbb) = (pull, 0.0, pull);
            for &(p, y) in points {
                let x = logit(p);
                let q = sigmoid(w.a * x + w.b);
                let r = q - if y { 1.0 } else { 0.0 };
                let s = q * (1.0 - q);
                ga += r * x;
                gb += r;
                haa += s * x * x;
                hab += s * x;
                hbb += s;
            }
            let det = haa * hbb - hab * hab;
            if det.abs() < 1e-12 {
                break;
            }
            let (da, db) = ((hbb * ga - hab * gb) / det, (haa * gb - hab * ga) / det);
            let before = objective(&w);
            let mut t = 1.0;
            let next = loop {
                let cand = Self {
                    a: w.a - t * da,
                    b: w.b - t * db,
                };
                if objective(&cand) <= before || t < 1e-6 {
                    break cand;
                }
                t /= 2.0;
            };
            let moved = (next.a - w.a).abs() + (next.b - w.b).abs();
            w = next;
            if moved < 1e-10 {
                break;
            }
        }
        w
    }
}

/// The gate's threshold from the developer's costs: acting on a judgment
/// costs `(1 − p) · wrong` in expectation and asking costs `ask`, so act
/// only when `p ≥ 1 − ask / wrong`. A wrong decision worth ten questions
/// gives 0.9.
pub fn threshold(wrong: f64, ask: f64) -> f64 {
    if wrong <= 0.0 {
        return 0.0;
    }
    (1.0 - ask / wrong).clamp(0.0, 1.0)
}

/// Mean squared error of the probabilities (a proper scoring rule).
pub fn brier(points: &[Labelled]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    points
        .iter()
        .map(|&(p, y)| (p - if y { 1.0 } else { 0.0 }).powi(2))
        .sum::<f64>()
        / points.len() as f64
}

/// Expected calibration error over `bins` equal-width bins: the weighted
/// mean gap between the mean p and the share right in each bin.
pub fn ece(points: &[Labelled], bins: usize) -> f64 {
    if points.is_empty() || bins == 0 {
        return 0.0;
    }
    let mut sum = vec![(0.0f64, 0.0f64, 0usize); bins];
    for &(p, y) in points {
        let k = ((p * bins as f64) as usize).min(bins - 1);
        sum[k].0 += p;
        sum[k].1 += if y { 1.0 } else { 0.0 };
        sum[k].2 += 1;
    }
    sum.iter()
        .filter(|s| s.2 > 0)
        .map(|&(p, y, n)| n as f64 / points.len() as f64 * (y / n as f64 - p / n as f64).abs())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_leaves_p_alone() {
        for p in [0.1, 0.5, 0.9] {
            assert!((Platt::identity().apply(p) - p).abs() < 1e-9);
        }
    }

    #[test]
    fn the_threshold_comes_from_the_costs() {
        assert!((threshold(10.0, 1.0) - 0.9).abs() < 1e-12);
        assert_eq!(threshold(1.0, 1.0), 0.0);
        assert_eq!(threshold(1.0, 2.0), 0.0);
        assert_eq!(threshold(0.0, 1.0), 0.0);
    }

    #[test]
    fn an_underconfident_judge_is_pushed_up() {
        // Says 0.5 and is right 9 times in 10; says 0.3 and is wrong.
        let mut pts: Vec<Labelled> = (0..90).map(|_| (0.5, true)).collect();
        pts.extend((0..10).map(|_| (0.5, false)));
        pts.extend((0..20).map(|_| (0.3, false)));
        let m = Platt::fit(&pts, 1.0);
        assert!(m.apply(0.5) > 0.8, "0.5 maps to {}", m.apply(0.5));
        assert!(m.apply(0.3) < 0.3);
        let mapped: Vec<Labelled> = pts.iter().map(|&(p, y)| (m.apply(p), y)).collect();
        assert!(ece(&mapped, 5) < ece(&pts, 5));
        assert!(brier(&mapped) < brier(&pts));
    }

    #[test]
    fn an_overconfident_judge_is_pulled_down() {
        // Says 0.95 and is right once in four: a plain Newton step
        // oscillated here and pushed 0.95 up to 1.0.
        let pts: Vec<Labelled> = vec![(0.95, true), (0.95, false), (0.95, false), (0.95, false)];
        let m = Platt::fit(&pts, 1.0);
        assert!(m.apply(0.95) < 0.95, "0.95 maps to {}", m.apply(0.95));
        let mapped: Vec<Labelled> = pts.iter().map(|&(p, y)| (m.apply(p), y)).collect();
        assert!(brier(&mapped) < brier(&pts));
        // Many such labels take it close to the share right.
        let many: Vec<Labelled> = pts.iter().cycle().take(400).copied().collect();
        assert!((Platt::fit(&many, 1.0).apply(0.95) - 0.25).abs() < 0.05);
    }

    #[test]
    fn few_labels_move_the_map_little() {
        let m = Platt::fit(&[(0.5, true)], 1.0);
        assert!((m.apply(0.5) - 0.5).abs() < 0.3);
    }
}
