//! Derived laws: what entry contracts and arrow effects imply.
//!
//! Local contracts (an object's entry needs, an arrow's ensures/revokes)
//! are stated once; system-wide statements follow from them. This module
//! explores the exact reachable states `(object, tokens held)` from the
//! start objects, treating every case precondition as possibly true (it
//! over-approximates what cases can do, never what tokens can do), and
//! reports:
//!
//! Starting at an object counts as entering it: a walk may start only
//! where the entry contract holds with no tokens, so a start in the middle
//! of the graph cannot skip a contract. "Arrival" means arriving by an
//! arrow; must/may sets describe arrivals.
//!
//! - for each object, the tokens **every** walk holds on arrival (must)
//!   and the tokens **some** walk may hold (may);
//! - **dead** arrows: their source is reachable, but no walk can take them;
//! - whether a `via` invariant is **enforced at runtime** by the contracts,
//!   i.e. no walk can reach its target avoiding its gateways, even where
//!   the bare graph has such a path.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::category::{ArrowId, Category, Invariant, ObjId};

pub type Tokens = BTreeSet<String>;

#[derive(Clone, Debug)]
pub struct Laws {
    pub starts: Vec<ObjId>,
    /// Every token set a walk can hold on arriving by an arrow, per object.
    pub arrivals: BTreeMap<ObjId, BTreeSet<Tokens>>,
    /// Objects some walk can be at (started there or arrived).
    pub reached: BTreeSet<ObjId>,
    /// Arrows taken by at least one walk.
    pub used: BTreeSet<ArrowId>,
    /// Token sets a walk can be at each object with (started or arrived).
    pub states: BTreeMap<ObjId, BTreeSet<Tokens>>,
    /// How each explored state was first reached (breadth-first, so the
    /// reconstructed paths are shortest). Absent for start states.
    pred: BTreeMap<(ObjId, Tokens), (ObjId, Tokens, ArrowId)>,
    /// How each arrival state was first arrived at by an arrow.
    arrival_pred: BTreeMap<(ObjId, Tokens), (ObjId, Tokens, ArrowId)>,
}

/// Evidence for a claim about one token on arrival at one object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    /// Every arrival state holds the token: the exhaustive certificate.
    Must {
        arrival_states: usize,
        /// Issuers of the token some walk actually takes (every holding
        /// walk took one of them).
        granted_by: Vec<ArrowId>,
        /// Revokers some walk takes (every walk that took one re-acquired
        /// the token before arriving).
        revoked_by: Vec<ArrowId>,
    },
    /// Some arrival lacks the token: a counterexample path.
    NotAlways { counterexample: Vec<ArrowId> },
}

impl Laws {
    /// A shortest path from a start to the state `(o, tokens)`, ending with
    /// an arrow into `o` when it is an arrival state.
    pub fn path_to(&self, o: ObjId, tokens: &Tokens) -> Option<Vec<ArrowId>> {
        let (mut obj, mut tok, last) = self.arrival_pred.get(&(o, tokens.clone())).cloned()?;
        let mut path = vec![last];
        while let Some((po, pt, a)) = self.pred.get(&(obj, tok.clone())).cloned() {
            path.push(a);
            obj = po;
            tok = pt;
        }
        path.reverse();
        Some(path)
    }

    /// A witness that some walk arrives at `o` holding `token`.
    pub fn witness(&self, o: ObjId, token: &str) -> Option<Vec<ArrowId>> {
        let t = self.arrivals.get(&o)?.iter().find(|t| t.contains(token))?;
        self.path_to(o, t)
    }

    /// Proof or counterexample for "every walk arriving at `o` holds
    /// `token`". `None` if nothing arrives at `o`.
    pub fn prove(&self, cat: &Category, o: ObjId, token: &str) -> Option<Proof> {
        let arrivals = self.arrivals.get(&o)?;
        if let Some(lacking) = arrivals.iter().find(|t| !t.contains(token)) {
            return Some(Proof::NotAlways {
                counterexample: self.path_to(o, lacking).unwrap_or_default(),
            });
        }
        let by = |f: fn(&crate::category::Arrow) -> &Vec<String>| {
            self.used
                .iter()
                .copied()
                .filter(|a| f(cat.arrow(*a)).iter().any(|t| t == token))
                .collect()
        };
        Some(Proof::Must {
            arrival_states: arrivals.len(),
            granted_by: by(|a| &a.ensures),
            revoked_by: by(|a| &a.revokes),
        })
    }

    /// Tokens every walk holds on arriving at `o` (`None`: unreachable).
    pub fn must(&self, o: ObjId) -> Option<Tokens> {
        let mut sets = self.arrivals.get(&o)?.iter();
        let first = sets.next()?.clone();
        Some(sets.fold(first, |acc, s| acc.intersection(s).cloned().collect()))
    }

    /// Tokens some walk may hold on arriving at `o`.
    pub fn may(&self, o: ObjId) -> Tokens {
        self.arrivals
            .get(&o)
            .into_iter()
            .flatten()
            .flatten()
            .cloned()
            .collect()
    }

    /// Arrows whose source is reachable but which no walk can take.
    pub fn dead(&self, cat: &Category) -> Vec<ArrowId> {
        (0..cat.arrows().len() as u32)
            .map(ArrowId)
            .filter(|a| self.reached.contains(&cat.arrow(*a).src) && !self.used.contains(a))
            .collect()
    }
}

/// Starts are the given objects, or, if none, every object a walk may
/// start at: those whose entry contract needs no tokens.
pub fn derive(cat: &Category, starts: &[ObjId]) -> Laws {
    let starts = if starts.is_empty() {
        startable(cat)
    } else {
        starts.to_vec()
    };
    let e = explore(cat, &starts, &[]);
    Laws {
        starts,
        reached: e.states.keys().copied().collect(),
        arrivals: e.arrivals,
        used: e.used,
        states: e.states,
        pred: e.pred,
        arrival_pred: e.arrival_pred,
    }
}

/// Objects a walk holding no tokens may start at.
pub fn startable(cat: &Category) -> Vec<ObjId> {
    (0..cat.objects().len() as u32)
        .map(ObjId)
        .filter(|o| cat.object(*o).entry.needs.is_empty())
        .collect()
}

/// Whether any walk from `from` (holding no tokens) can reach a different
/// object `to` without visiting `avoid`, honouring entry contracts and
/// effects. (`from == to` is never a bypass.)
pub fn walkable(cat: &Category, from: ObjId, to: ObjId, avoid: &[ObjId]) -> bool {
    from != to && explore(cat, &[from], avoid).arrivals.contains_key(&to)
}

/// For each `via` invariant: `Some(true)` if the contracts enforce it for
/// every walk, `Some(false)` if some walk can bypass it, `None` for other
/// kinds or unknown objects.
pub fn enforced(cat: &Category, inv: &Invariant) -> Option<bool> {
    let Invariant::Via { from, to, through } = inv else {
        return None;
    };
    let id = |n: &str| cat.object_id(n).ok();
    let avoid: Vec<ObjId> = through.iter().filter_map(|n| id(n)).collect();
    Some(!walkable(cat, id(from)?, id(to)?, &avoid))
}

struct Exploration {
    arrivals: BTreeMap<ObjId, BTreeSet<Tokens>>,
    states: BTreeMap<ObjId, BTreeSet<Tokens>>,
    used: BTreeSet<ArrowId>,
    pred: BTreeMap<(ObjId, Tokens), (ObjId, Tokens, ArrowId)>,
    arrival_pred: BTreeMap<(ObjId, Tokens), (ObjId, Tokens, ArrowId)>,
}

fn explore(cat: &Category, starts: &[ObjId], avoid: &[ObjId]) -> Exploration {
    let mut e = Exploration {
        arrivals: BTreeMap::new(),
        states: BTreeMap::new(),
        used: BTreeSet::new(),
        pred: BTreeMap::new(),
        arrival_pred: BTreeMap::new(),
    };
    let mut queue: VecDeque<(ObjId, Tokens)> = VecDeque::new();
    for &s in starts.iter().filter(|s| !avoid.contains(s)) {
        // Starting is entering: only where the contract needs no tokens.
        if cat.object(s).entry.needs.is_empty()
            && e.states.entry(s).or_default().insert(Tokens::new())
        {
            queue.push_back((s, Tokens::new()));
        }
    }
    while let Some((o, tokens)) = queue.pop_front() {
        for &a in cat.out(o) {
            let arrow = cat.arrow(a);
            if avoid.contains(&arrow.dst) {
                continue;
            }
            let after = arrow.effect(&tokens);
            // Case preconditions are assumed satisfiable; token needs are not.
            if !cat
                .object(arrow.dst)
                .entry
                .needs
                .iter()
                .all(|t| after.contains(t))
            {
                continue;
            }
            e.used.insert(a);
            if e.arrivals
                .entry(arrow.dst)
                .or_default()
                .insert(after.clone())
            {
                e.arrival_pred
                    .insert((arrow.dst, after.clone()), (o, tokens.clone(), a));
            }
            if e.states.entry(arrow.dst).or_default().insert(after.clone()) {
                e.pred
                    .insert((arrow.dst, after.clone()), (o, tokens.clone(), a));
                queue.push_back((arrow.dst, after));
            }
        }
    }
    e
}
