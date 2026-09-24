//! Functor discovery (`docs/05-functors.md` §7): finding a candidate map
//! from one category into another.
//!
//! Structure decides what is possible; evidence ranks what is possible.
//! [`admissible`] keeps, per source object, the targets its structural
//! role allows. The caller scores those pairs (by meaning, a judge; by
//! behaviour, where the same cases went). [`search`] finds the object map
//! with the best total score under which every arrow between mapped
//! objects has a path, and [`arrow_images`] lists the paths each arrow may
//! map to. The result is a declaration ([`declaration`]), checked like any
//! functor ([`crate::functor::Functor::build`]): a discovered functor is a
//! proposal, never policy until a person adopts it.

use std::collections::BTreeSet;

use crate::category::{Category, ObjId, PathSpec};
use crate::functor::FunctorDecl;
use crate::path::Path;

/// Entry points: declared starts, else objects no arrow enters.
pub fn entries(cat: &Category) -> Vec<ObjId> {
    if !cat.starts().is_empty() {
        return cat.starts().to_vec();
    }
    let entered: BTreeSet<ObjId> = cat.arrows().iter().map(|a| a.dst).collect();
    (0..cat.objects().len() as u32)
        .map(ObjId)
        .filter(|o| !entered.contains(o))
        .collect()
}

/// Per source object, the target objects its structural role allows:
/// an entry maps to an entry, a terminal (no way on) to a terminal, a
/// decision to a decision of the same primitive.
pub fn admissible(a: &Category, b: &Category) -> Vec<Vec<ObjId>> {
    let (ea, eb) = (entries(a), entries(b));
    let terminal = |c: &Category, o: ObjId| c.out(o).is_empty();
    (0..a.objects().len() as u32)
        .map(ObjId)
        .map(|x| {
            (0..b.objects().len() as u32)
                .map(ObjId)
                .filter(|y| {
                    if ea.contains(&x) != eb.contains(y) {
                        return false;
                    }
                    if terminal(a, x) != terminal(b, *y) {
                        return false;
                    }
                    terminal(a, x) || a.object(x).frame.primitive == b.object(*y).frame.primitive
                })
                .collect()
        })
        .collect()
}

/// The object map found, with its total score and how much was searched.
#[derive(Clone, Debug)]
pub struct Found {
    /// Per source object: its image, or `None` outside the domain.
    pub objects: Vec<Option<ObjId>>,
    pub score: f32,
    /// Assignments tried (the search is bounded).
    pub explored: usize,
    /// Candidate pairs rejected because some arrow had no path.
    pub pruned: usize,
}

/// The best-scoring object map under which every arrow between mapped
/// objects has a path of length at most `max_len` in the target (or maps
/// to an identity when both ends share an image, which `injective`
/// forbids). `scores[x]` ranks the candidates for `x`; an object may
/// always stay outside the domain, scoring `outside[x]`.
pub fn search(
    a: &Category,
    b: &Category,
    scores: &[Vec<(ObjId, f32)>],
    outside: &[f32],
    injective: bool,
    max_len: usize,
) -> Found {
    // Parents before children: a breadth-first order from the entries.
    let mut order: Vec<ObjId> = Vec::new();
    let mut queue: std::collections::VecDeque<ObjId> = entries(a).into();
    while let Some(x) = queue.pop_front() {
        if order.contains(&x) {
            continue;
        }
        order.push(x);
        for f in a.out(x) {
            queue.push_back(a.arrow(*f).dst);
        }
    }
    for x in (0..a.objects().len() as u32).map(ObjId) {
        if !order.contains(&x) {
            order.push(x);
        }
    }
    let n = order.len();
    // The best each remaining object could still add (for the bound).
    let best_of = |x: ObjId| {
        scores[x.0 as usize]
            .iter()
            .map(|c| c.1)
            .fold(outside[x.0 as usize], f32::max)
    };
    let mut rest = vec![0.0f32; n + 1];
    for i in (0..n).rev() {
        rest[i] = rest[i + 1] + best_of(order[i]);
    }

    struct S<'a> {
        a: &'a Category,
        b: &'a Category,
        scores: &'a [Vec<(ObjId, f32)>],
        outside: &'a [f32],
        order: Vec<ObjId>,
        rest: Vec<f32>,
        injective: bool,
        max_len: usize,
        map: Vec<Option<ObjId>>,
        best: Option<(f32, Vec<Option<ObjId>>)>,
        explored: usize,
        pruned: usize,
    }
    impl S<'_> {
        /// Every arrow between `x` and an already mapped object has a path.
        fn fits(&self, x: ObjId, y: ObjId) -> bool {
            if self.injective && self.map.contains(&Some(y)) {
                return false;
            }
            let ok = |from: ObjId, to: ObjId| {
                (from == to && !self.injective)
                    || !self.b.paths(from, to, &[], self.max_len).is_empty()
            };
            self.a.arrows().iter().all(|f| {
                let (s, d) = (f.src, f.dst);
                let img = |o: ObjId| {
                    if o == x {
                        Some(y)
                    } else {
                        self.map[o.0 as usize]
                    }
                };
                if s != x && d != x {
                    return true;
                }
                match (img(s), img(d)) {
                    (Some(p), Some(q)) => ok(p, q),
                    _ => true,
                }
            })
        }
        fn go(&mut self, i: usize, score: f32) {
            if self.explored > 200_000 {
                return;
            }
            if let Some((b, _)) = &self.best
                && score + self.rest[i] <= *b
            {
                return;
            }
            if i == self.order.len() {
                self.best = Some((score, self.map.clone()));
                return;
            }
            let x = self.order[i];
            let mut options: Vec<(Option<ObjId>, f32)> = self.scores[x.0 as usize]
                .iter()
                .map(|(y, s)| (Some(*y), *s))
                .collect();
            options.push((None, self.outside[x.0 as usize]));
            options.sort_by(|p, q| q.1.total_cmp(&p.1));
            for (y, s) in options {
                self.explored += 1;
                if let Some(y) = y
                    && !self.fits(x, y)
                {
                    self.pruned += 1;
                    continue;
                }
                self.map[x.0 as usize] = y;
                self.go(i + 1, score + s);
                self.map[x.0 as usize] = None;
            }
        }
    }
    let mut s = S {
        a,
        b,
        scores,
        outside,
        order,
        rest,
        injective,
        max_len,
        map: vec![None; a.objects().len()],
        best: None,
        explored: 0,
        pruned: 0,
    };
    s.go(0, 0.0);
    let (score, objects) = s.best.unwrap_or((0.0, vec![None; a.objects().len()]));
    Found {
        objects,
        score,
        explored: s.explored,
        pruned: s.pruned,
    }
}

/// Per source arrow between mapped objects: the target paths it may map
/// to, shortest first (an identity when both ends share an image).
/// `None` outside the domain.
pub fn arrow_images(
    a: &Category,
    b: &Category,
    objects: &[Option<ObjId>],
    max_len: usize,
) -> Vec<Option<Vec<Path>>> {
    a.arrows()
        .iter()
        .map(|f| {
            let (p, q) = (objects[f.src.0 as usize]?, objects[f.dst.0 as usize]?);
            if p == q {
                return Some(vec![Path::id(p)]);
            }
            let all = b.paths(p, q, &[], max_len);
            let shortest = all.first().map_or(0, |x| x.arrows.len());
            Some(
                all.into_iter()
                    .filter(|x| x.arrows.len() == shortest)
                    .collect(),
            )
        })
        .collect()
}

/// The declaration of a discovered functor (`require:` left empty: the
/// report says what it keeps and reflects; a person decides).
pub fn declaration(
    name: &str,
    a: &Category,
    b: &Category,
    objects: &[Option<ObjId>],
    arrows: &[Option<Path>],
) -> FunctorDecl {
    FunctorDecl {
        name: name.to_owned(),
        src: a.name().to_owned(),
        dst: b.name().to_owned(),
        objects: objects
            .iter()
            .enumerate()
            .filter_map(|(i, y)| Some((a.objects()[i].name.clone(), b.object((*y)?).name.clone())))
            .collect(),
        arrows: arrows
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let p = p.as_ref()?;
                let spec = if p.is_id() {
                    PathSpec::Arrows(vec!["id".into()])
                } else {
                    PathSpec::Arrows(p.arrows.iter().map(|g| b.arrow(*g).name.clone()).collect())
                };
                Some((a.arrows()[i].name.clone(), spec))
            })
            .collect(),
        ..FunctorDecl::default()
    }
}

/// The declaration as `.onto` text.
pub fn render(d: &FunctorDecl, notes: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = format!("functor {}: {} -> {} {{\n", d.name, d.src, d.dst);
    out.push_str("    objects:\n");
    for (i, (x, y)) in d.objects.iter().enumerate() {
        let sep = if i + 1 == d.objects.len() { ";" } else { "," };
        let note = notes(x).map_or(String::new(), |n| format!("    # {n}"));
        out.push_str(&format!("        {x} -> {y}{sep}{note}\n"));
    }
    for (f, spec) in &d.arrows {
        let shown = match spec {
            PathSpec::Id(o) => format!("id({o})"),
            PathSpec::Arrows(v) => v.iter().rev().cloned().collect::<Vec<_>>().join("."),
        };
        out.push_str(&format!("    {f}: {shown};\n"));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = r#"category A {
        objects: In, Money, Tech, Refund, Crash, Done;
        frame In: choice "which team?";
        money: In -> Money; tech: In -> Tech;
        refund: Money -> Refund; crash: Tech -> Crash;
        pay: Refund -> Done; fix: Crash -> Done;
    }"#;
    const B: &str = r#"category B {
        objects: Inbox, Payments, Support, Orders, Chargeback, Incident, Track, Closed;
        frame Inbox: choice "which queue?";
        payments: Inbox -> Payments; support: Inbox -> Support; orders: Inbox -> Orders;
        chargeback: Payments -> Chargeback; incident: Support -> Incident; track: Orders -> Track;
        repay: Chargeback -> Closed; resolve: Incident -> Closed; deliver: Track -> Closed;
    }"#;

    fn cats() -> (Category, Category) {
        (crate::parse(A).unwrap(), crate::parse(B).unwrap())
    }

    fn uniform(
        a: &Category,
        adm: &[Vec<ObjId>],
        prefer: &[(&str, &str)],
        b: &Category,
    ) -> Vec<Vec<(ObjId, f32)>> {
        adm.iter()
            .enumerate()
            .map(|(i, ys)| {
                ys.iter()
                    .map(|y| {
                        let hit = prefer
                            .iter()
                            .any(|(x, t)| a.objects()[i].name == *x && b.object(*y).name == *t);
                        (*y, if hit { 0.9 } else { 0.3 })
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn structure_restricts_roles() {
        let (a, b) = cats();
        let adm = admissible(&a, &b);
        let names = |x: &str| -> Vec<String> {
            adm[a.object_id(x).unwrap().0 as usize]
                .iter()
                .map(|y| b.object(*y).name.clone())
                .collect()
        };
        assert_eq!(names("In"), ["Inbox"]);
        assert_eq!(names("Done"), ["Closed"]);
        assert!(!names("Money").contains(&"Closed".to_owned()));
    }

    #[test]
    fn evidence_picks_among_what_structure_allows() {
        let (a, b) = cats();
        let adm = admissible(&a, &b);
        let scores = uniform(
            &a,
            &adm,
            &[
                ("Money", "Payments"),
                ("Tech", "Support"),
                ("Refund", "Chargeback"),
                ("Crash", "Incident"),
            ],
            &b,
        );
        let f = search(&a, &b, &scores, &vec![0.1; a.objects().len()], true, 3);
        let img = |x: &str| {
            f.objects[a.object_id(x).unwrap().0 as usize].map(|y| b.object(y).name.clone())
        };
        assert_eq!(img("Money").as_deref(), Some("Payments"));
        assert_eq!(img("Refund").as_deref(), Some("Chargeback"));
        assert_eq!(img("Crash").as_deref(), Some("Incident"));
        // Every arrow has exactly one image here; the declaration checks.
        let arrows: Vec<Option<Path>> = arrow_images(&a, &b, &f.objects, 3)
            .into_iter()
            .map(|v| v.and_then(|v| v.into_iter().next()))
            .collect();
        let d = declaration("F", &a, &b, &f.objects, &arrows);
        let built = crate::functor::Functor::build(&d, &a, &b).unwrap();
        assert!(
            built
                .report
                .uncovered_objects
                .contains(&"Orders".to_owned())
        );
    }

    #[test]
    fn a_preferred_pair_with_no_path_is_pruned() {
        // Evidence prefers Refund -> Incident, but Money -> Payments leaves
        // no path Payments -> Incident: structure overrules the score.
        let (a, b) = cats();
        let adm = admissible(&a, &b);
        let mut scores = uniform(&a, &adm, &[("Money", "Payments")], &b);
        let refund = a.object_id("Refund").unwrap().0 as usize;
        for (y, s) in &mut scores[refund] {
            if b.object(*y).name == "Incident" {
                *s = 1.0;
            }
        }
        let money = a.object_id("Money").unwrap().0 as usize;
        for (y, s) in &mut scores[money] {
            if b.object(*y).name == "Payments" {
                *s = 5.0;
            }
        }
        let f = search(&a, &b, &scores, &vec![0.1; a.objects().len()], true, 3);
        let img = |x: &str| {
            f.objects[a.object_id(x).unwrap().0 as usize].map(|y| b.object(y).name.clone())
        };
        assert_eq!(img("Money").as_deref(), Some("Payments"));
        assert_eq!(img("Refund").as_deref(), Some("Chargeback"));
        assert!(f.pruned > 0);
    }
}
