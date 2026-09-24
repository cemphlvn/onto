//! Ensembles: several categories walk the same case, each a column with a
//! functor into one shared category (`docs/05-functors.md` §6).
//!
//! Two positions in the shared category are **compatible** when equal or
//! when one reaches the other (one perspective is further along); they are
//! a **surprise** when neither reaches the other. The shared category's
//! structure, declared by people, defines contradiction.

use crate::category::{Category, ObjId};
use crate::error::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consensus {
    /// Every column's final position is pairwise compatible.
    All,
    /// At least N columns form a compatible set; the rest dissent.
    Quorum(usize),
}

#[derive(Clone, Debug)]
pub struct Column {
    /// The column's category (and its name).
    pub category: String,
    /// A functor from the column's category into the shared category.
    pub functor: String,
    /// Where the column's walk starts.
    pub start: String,
}

#[derive(Clone, Debug)]
pub struct Ensemble {
    pub name: String,
    pub shared: String,
    pub columns: Vec<Column>,
    pub consensus: Consensus,
}

impl Ensemble {
    /// `shared: X; column C: F from S; consensus: all | quorum N;`
    pub fn parse(text: &str) -> Result<Self, String> {
        let (name, body) = text
            .split_once('{')
            .ok_or("expected `ensemble Name { … }`")?;
        let body = body.trim().strip_suffix('}').ok_or("missing `}`")?;
        let mut e = Ensemble {
            name: name.trim().to_owned(),
            shared: String::new(),
            columns: Vec::new(),
            consensus: Consensus::All,
        };
        for item in body.split(';').map(str::trim).filter(|i| !i.is_empty()) {
            if let Some(rest) = item.strip_prefix("column ") {
                let (cat, rest) = rest
                    .split_once(':')
                    .ok_or("expected `column C: F from S`")?;
                let (functor, start) = rest
                    .split_once(" from ")
                    .ok_or("expected `column C: F from S`")?;
                e.columns.push(Column {
                    category: cat.trim().to_owned(),
                    functor: functor.trim().to_owned(),
                    start: start.trim().to_owned(),
                });
                continue;
            }
            let (k, v) = item
                .split_once(':')
                .ok_or_else(|| format!("cannot read `{item}`"))?;
            match (k.trim(), v.trim()) {
                ("shared", v) => e.shared = v.to_owned(),
                ("consensus", "all") => e.consensus = Consensus::All,
                ("consensus", v) if v.starts_with("quorum") => {
                    let n = v["quorum".len()..]
                        .trim()
                        .parse()
                        .map_err(|_| format!("expected `quorum N`, got `{v}`"))?;
                    e.consensus = Consensus::Quorum(n);
                }
                (k, v) => return Err(format!("unknown `{k}: {v}` (shared, column, consensus)")),
            }
        }
        if e.shared.is_empty() {
            return Err("an ensemble needs `shared: Category`".into());
        }
        if e.columns.len() < 2 {
            return Err("an ensemble needs at least two columns".into());
        }
        Ok(e)
    }

    /// Checks the ensemble against its module.
    pub fn validate(&self, m: &crate::parse::Module) -> Result<(), Error> {
        let bad = |msg: String| Error::Parse {
            line: 0,
            msg: format!("ensemble {}: {msg}", self.name),
        };
        m.category(&self.shared)
            .ok_or_else(|| bad(format!("no shared category `{}`", self.shared)))?;
        for c in &self.columns {
            let cat = m
                .category(&c.category)
                .ok_or_else(|| bad(format!("no category `{}`", c.category)))?;
            let f = m
                .functor(&c.functor)
                .ok_or_else(|| bad(format!("no functor `{}`", c.functor)))?;
            if f.src != c.category || f.dst != self.shared {
                return Err(bad(format!(
                    "functor {} runs {} -> {}, not {} -> {}",
                    f.name, f.src, f.dst, c.category, self.shared
                )));
            }
            let start = cat.object_id(&c.start).map_err(|e| bad(e.to_string()))?;
            if f.object(start).is_none() {
                return Err(bad(format!("{} is outside {}'s domain", c.start, f.name)));
            }
        }
        if let Consensus::Quorum(n) = self.consensus
            && (n < 1 || n > self.columns.len())
        {
            return Err(bad(format!(
                "quorum {n} with {} columns",
                self.columns.len()
            )));
        }
        Ok(())
    }
}

/// Two positions are compatible when one reaches the other.
pub fn compatible(shared: &Category, a: ObjId, b: ObjId) -> bool {
    a == b || shared.reachable(a, b, &[]).is_some() || shared.reachable(b, a, &[]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
category S { objects: O, X, Y; x: O -> X; y: O -> Y; }
category A { objects: A0, A1; start: A0; a: A0 -> A1; }
category B { objects: B0, B1; start: B0; b: B0 -> B1; }
functor FA: A -> S { objects: A0 -> O, A1 -> X; a: x; }
functor FB: B -> S { objects: B0 -> O, B1 -> Y; b: y; }
ensemble E {
    shared: S;
    column A: FA from A0;
    column B: FB from B0;
    consensus: all;
}
"#;

    fn module(src: &str) -> Result<crate::parse::Module, Error> {
        crate::parse::parse_module(src, &mut |p| {
            Err(Error::Parse {
                line: 0,
                msg: format!("no import {p}"),
            })
        })
    }

    #[test]
    fn parses_and_compares_by_reachability() {
        let m = module(SRC).unwrap();
        let e = m.ensemble("E").unwrap();
        assert_eq!(e.columns.len(), 2);
        assert_eq!(e.consensus, Consensus::All);
        let s = m.category("S").unwrap();
        let id = |n: &str| s.object_id(n).unwrap();
        assert!(compatible(s, id("O"), id("X")));
        assert!(compatible(s, id("Y"), id("O")));
        assert!(!compatible(s, id("X"), id("Y")));
    }

    #[test]
    fn columns_must_map_into_the_shared_category_from_their_start() {
        let wrong_target = SRC.replace(
            "functor FB: B -> S",
            "category T { objects: O, Y; y: O -> Y; }\nfunctor FB: B -> T",
        );
        let e = module(&wrong_target).unwrap_err().to_string();
        assert!(e.contains("not B -> S"), "{e}");
        let later = SRC.replace("column B: FB from B0;", "column B: FB from B1;");
        assert!(module(&later).is_ok(), "B1 is mapped: a valid start");
        let outside = SRC
            .replace("objects: B0, B1;", "objects: B0, B1, B2;")
            .replace("column B: FB from B0;", "column B: FB from B2;");
        let e = module(&outside).unwrap_err().to_string();
        assert!(e.contains("outside"), "{e}");
        let quorum = SRC.replace("consensus: all;", "consensus: quorum 3;");
        let e = module(&quorum).unwrap_err().to_string();
        assert!(e.contains("quorum 3"), "{e}");
    }
}
