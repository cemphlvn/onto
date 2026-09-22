//! Path equality by equality saturation (egg).
//!
//! Paths become terms `(o g f)` / `(id A)`; the category laws (associativity,
//! identities) and the declared equations become rewrites. Two paths are equal
//! when saturation puts them in the same e-class. The word problem for
//! finitely presented categories is undecidable in general, so a run that hits
//! its limits answers [`Verdict::Unknown`] instead of guessing.

use egg::{AstSize, Extractor, Id, Pattern, RecExpr, Rewrite, Runner, StopReason, Symbol};

use crate::category::Category;
use crate::error::Error;
use crate::path::Path;

egg::define_language! {
    pub enum Mor {
        "o" = Comp([Id; 2]),
        "id" = Ident(Id),
        Symbol(Symbol),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Proven equal from the laws and declared equations.
    Equal,
    /// Saturated without merging: not equal in the presented category.
    Distinct,
    /// Limits reached before either could be established.
    Unknown,
}

pub struct Equality<'c> {
    cat: &'c Category,
    rules: Vec<Rewrite<Mor, ()>>,
    iter_limit: usize,
    node_limit: usize,
}

impl<'c> Equality<'c> {
    pub fn new(cat: &'c Category) -> Result<Self, Error> {
        let mut rules: Vec<Rewrite<Mor, ()>> = Vec::new();
        rules.extend(egg::rewrite!("assoc"; "(o ?h (o ?g ?f))" <=> "(o (o ?h ?g) ?f)"));
        // Only well-typed terms ever enter the e-graph, so `(o ?f (id ?a))`
        // implies dom(f) = a and the identity laws need no side condition.
        rules.push(egg::rewrite!("id-left"; "(o (id ?a) ?f)" => "?f"));
        rules.push(egg::rewrite!("id-right"; "(o ?f (id ?a))" => "?f"));

        for (i, eq) in cat.equations().iter().enumerate() {
            let lhs = pattern(cat, &eq.lhs)?;
            let rhs = pattern(cat, &eq.rhs)?;
            for (name, l, r) in [
                (format!("eq{i}"), lhs.clone(), rhs.clone()),
                (format!("eq{i}-rev"), rhs, lhs),
            ] {
                rules.push(Rewrite::new(name, l, r).map_err(Error::Rewrite)?);
            }
        }

        Ok(Self {
            cat,
            rules,
            iter_limit: 32,
            node_limit: 20_000,
        })
    }

    pub fn with_limits(mut self, iter_limit: usize, node_limit: usize) -> Self {
        self.iter_limit = iter_limit;
        self.node_limit = node_limit;
        self
    }

    pub fn check(&self, a: &Path, b: &Path) -> Verdict {
        if (a.src, a.dst) != (b.src, b.dst) {
            return Verdict::Distinct;
        }
        let runner = self
            .runner()
            .with_expr(&to_expr(self.cat, a))
            .with_expr(&to_expr(self.cat, b))
            .run(&self.rules);
        let eg = &runner.egraph;
        if eg.find(runner.roots[0]) == eg.find(runner.roots[1]) {
            Verdict::Equal
        } else if matches!(runner.stop_reason, Some(StopReason::Saturated)) {
            Verdict::Distinct
        } else {
            Verdict::Unknown
        }
    }

    /// The shortest path found equal to `p`.
    pub fn simplest(&self, p: &Path) -> Path {
        let runner = self
            .runner()
            .with_expr(&to_expr(self.cat, p))
            .run(&self.rules);
        let (_, best) = Extractor::new(&runner.egraph, AstSize).find_best(runner.roots[0]);
        from_expr(self.cat, &best, p)
    }

    fn runner(&self) -> Runner<Mor, ()> {
        Runner::default()
            .with_iter_limit(self.iter_limit)
            .with_node_limit(self.node_limit)
    }
}

/// `[f, g, h]` (application order) becomes `(o h (o g f))`.
fn to_expr(cat: &Category, p: &Path) -> RecExpr<Mor> {
    let mut e = RecExpr::default();
    if p.is_id() {
        let obj = e.add(Mor::Symbol(cat.object(p.src).name.as_str().into()));
        e.add(Mor::Ident(obj));
        return e;
    }
    let mut acc = e.add(Mor::Symbol(cat.arrow(p.arrows[0]).name.as_str().into()));
    for a in &p.arrows[1..] {
        let next = e.add(Mor::Symbol(cat.arrow(*a).name.as_str().into()));
        acc = e.add(Mor::Comp([next, acc]));
    }
    e
}

fn pattern(cat: &Category, p: &Path) -> Result<Pattern<Mor>, Error> {
    to_expr(cat, p)
        .to_string()
        .parse()
        .map_err(|e| Error::Rewrite(format!("{e}")))
}

/// Flattens an extracted term back into a path typed like `original`.
fn from_expr(cat: &Category, e: &RecExpr<Mor>, original: &Path) -> Path {
    fn walk(cat: &Category, e: &RecExpr<Mor>, id: Id, out: &mut Vec<crate::ArrowId>) {
        match &e[id] {
            // `(o g f)`: f is applied first.
            Mor::Comp([g, f]) => {
                walk(cat, e, *f, out);
                walk(cat, e, *g, out);
            }
            Mor::Ident(_) => {}
            Mor::Symbol(s) => {
                out.push(
                    cat.arrow_id(s.as_str())
                        .expect("extracted a declared arrow"),
                );
            }
        }
    }
    let mut arrows = Vec::new();
    walk(cat, e, e.root(), &mut arrows);
    Path {
        src: original.src,
        dst: original.dst,
        arrows,
    }
}
