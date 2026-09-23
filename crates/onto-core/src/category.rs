//! The category store: objects, arrows, path equations and the
//! per-object decision frames (outgoing arrows, laid out as CSR).

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use crate::error::Error;
use crate::path::Path;
use crate::require::Require;

/// Index of an object. Dense, `0..category.objects().len()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjId(pub u32);

/// Index of an arrow (morphism). Dense, `0..category.arrows().len()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArrowId(pub u32);

/// Whether an object's outgoing arrows are a verified MECE enumeration.
///
/// A `Closed` frame lets the System-1 chooser decide on its own. An `Open`
/// frame is known to be incomplete, so every step from it escalates to the
/// System-2 proposer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Closure {
    Closed,
    #[default]
    Open,
}

/// Which System-1 primitive decides an object's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum Primitive {
    /// Exactly one arrow (or none of these): exclusive frames.
    #[default]
    Choice,
    /// Each arrow's condition judged on its own; several may hold.
    Noul,
    /// One ordered scale; each arrow is a level on it.
    Score,
    /// No judgment: every eligible arrow is pursued (an AND-split), e.g.
    /// mandatory verification checks. A model must not be able to drop one.
    Split,
}

impl Primitive {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::Noul => "noul",
            Self::Score => "score",
            Self::Split => "split",
        }
    }
}

/// How an object's frame is decided: the primitive, and optionally the
/// question put to the model (text or structured JSON).
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub primitive: Primitive,
    pub instructions: Option<Value>,
    /// Noul frames only: every arrow that holds is pursued as a concurrent
    /// alternative (no fork question), e.g. mitigation plans racing to a
    /// `race` join. The branch budget still applies.
    pub parallel: bool,
}

/// What every walk entering an object must hold: capability tokens it
/// needs, and a precondition over the case. Inherited by every incoming
/// arrow, including arrows added later.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub needs: Vec<String>,
    pub require: Option<Require>,
}

impl Entry {
    pub fn is_empty(&self) -> bool {
        self.needs.is_empty() && self.require.is_none()
    }
}

/// How sibling branches of one fork recombine at an object
/// (`docs/03-joins.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Join {
    /// Wait for every sibling; continue with the intersection of tokens.
    All,
    /// The first sibling to arrive continues with its own tokens.
    Race,
    /// Content continues once the branch spawned along `authority` has
    /// arrived, with (content ∩ authority) ∪ (authority ∩ `export`).
    Gate {
        authority: String,
        export: Vec<String>,
    },
}

impl Join {
    pub fn name(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Race => "race",
            Self::Gate { .. } => "gate",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Object {
    pub name: String,
    pub closure: Closure,
    /// What this object means (text or structured JSON).
    pub about: Option<Value>,
    pub frame: Frame,
    pub entry: Entry,
    /// Set when sibling branches recombine here.
    pub join: Option<Join>,
}

/// Whether an arrow may be taken from a given case and token set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    Open,
    /// The arrow's own `require` failed.
    Require,
    /// The arrow's `attested` precondition is not met by verified
    /// observations.
    Unattested,
    /// The target's entry contract failed: missing tokens (after the
    /// arrow's effects), and/or its case precondition.
    Entry {
        missing: Vec<String>,
        require_failed: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Arrow {
    pub name: String,
    pub src: ObjId,
    pub dst: ObjId,
    /// When to follow this arrow (text or structured JSON). Rendered by
    /// each model adapter as a Choice option, a Noul condition, or a Score level.
    pub instructions: Option<Value>,
    /// Position on the source's Score scale (score frames only).
    pub level: Option<u32>,
    /// Checked in code against the walk's state before any model call.
    pub require: Option<Require>,
    /// Checked against the case's **attested view** (verified, signed
    /// observations only), never against plain case facts.
    pub attested: Option<Require>,
    /// Capability tokens the walk holds after taking this arrow.
    pub ensures: Vec<String>,
    /// Capability tokens the walk loses on taking this arrow (applied first).
    pub revokes: Vec<String>,
    /// Added by the open world (a model's proposal that passed the
    /// supervisor's checks), not declared by a person.
    pub learned: bool,
}

impl Arrow {
    /// The token set after taking this arrow: revokes first, then ensures.
    pub fn effect(&self, tokens: &BTreeSet<String>) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = tokens
            .iter()
            .filter(|t| !self.revokes.contains(t))
            .cloned()
            .collect();
        out.extend(self.ensures.iter().cloned());
        out
    }
}

/// Optional meaning attached to an arrow at declaration.
#[derive(Clone, Debug, Default)]
pub struct ArrowMeta {
    pub instructions: Option<Value>,
    pub level: Option<u32>,
    pub require: Option<Require>,
    pub attested: Option<Require>,
    pub ensures: Vec<String>,
    pub revokes: Vec<String>,
    pub learned: bool,
}

/// A rule the graph must keep, checked on load and on every proposal.
/// Names are kept as written: an invariant may name an object that does
/// not exist yet (e.g. `never: Collected -> Advertiser`), so that a
/// proposal introducing it is caught.
#[derive(Clone, Debug, PartialEq)]
pub enum Invariant {
    /// Every path from `from` to `to` passes through at least one of
    /// `through` (written `through A | B`).
    Via {
        from: String,
        to: String,
        through: Vec<String>,
    },
    /// No path from `from` to `to`.
    Never { from: String, to: String },
    /// A natural-language rule, judged by a model (text or JSON).
    Rule(Value),
}

impl std::fmt::Display for Invariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Via { from, to, through } => {
                write!(f, "via: {from} -> {to} through {}", through.join(" | "))
            }
            Self::Never { from, to } => write!(f, "never: {from} -> {to}"),
            Self::Rule(v) => write!(f, "rule {v}"),
        }
    }
}

/// Which arrows may issue and revoke a capability token. Only these
/// arrows may `ensures` / `revokes` it: a token certifies that evidence
/// was checked by an authorized transition, not that some arrow ran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    pub issuers: Vec<String>,
    pub revokers: Vec<String>,
}

/// A declared equation between two parallel paths.
#[derive(Clone, Debug)]
pub struct Equation {
    pub lhs: Path,
    pub rhs: Path,
}

/// An immutable, validated category.
///
/// Outgoing arrows are stored contiguously per object (CSR): the decision
/// frame of object `o` is `out_arrows[out_offsets[o]..out_offsets[o + 1]]`.
#[derive(Clone, Debug)]
pub struct Category {
    name: String,
    /// SHA-256 of the source text this category was parsed from (hex);
    /// `None` for categories built in code or extended hypothetically.
    snapshot: Option<String>,
    objects: Vec<Object>,
    arrows: Vec<Arrow>,
    equations: Vec<Equation>,
    invariants: Vec<Invariant>,
    capabilities: Vec<Capability>,
    attesters: Vec<crate::attest::Attester>,
    /// Declared application entry points (`start: A, B;`).
    starts: Vec<ObjId>,
    out_offsets: Vec<u32>,
    out_arrows: Vec<ArrowId>,
    object_index: HashMap<String, ObjId>,
    arrow_index: HashMap<String, ArrowId>,
}

impl Category {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The snapshot this category was loaded from: the SHA-256 of its
    /// source text, verifiable with `shasum -a 256 file.onto`.
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref()
    }

    pub(crate) fn set_snapshot(&mut self, hash: String) {
        self.snapshot = Some(hash);
    }

    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn arrows(&self) -> &[Arrow] {
        &self.arrows
    }

    pub fn equations(&self) -> &[Equation] {
        &self.equations
    }

    pub fn invariants(&self) -> &[Invariant] {
        &self.invariants
    }

    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    /// Declared attesters: whose signed observations count, and for which
    /// fields. Part of the policy.
    pub fn attesters(&self) -> &[crate::attest::Attester] {
        &self.attesters
    }

    pub fn capability(&self, name: &str) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.name == name)
    }

    /// Declared application entry points.
    pub fn starts(&self) -> &[ObjId] {
        &self.starts
    }

    /// The capability rules every route into the graph shares (loading,
    /// the supervisor's hypothetical extension, promotion): every token
    /// used is declared once; only a declared issuer may `ensures` it and
    /// only a declared revoker may `revokes` it; no arrow both issues and
    /// revokes the same token; declarations name existing arrows; declared
    /// starts can be entered holding nothing.
    pub fn validate_capabilities(&self) -> Result<(), Error> {
        let bad = |msg: String| Err(Error::Capability(msg));
        let mut seen = BTreeSet::new();
        for c in &self.capabilities {
            if !seen.insert(c.name.as_str()) {
                return bad(format!("capability {} is declared twice", c.name));
            }
            for a in c.issuers.iter().chain(&c.revokers) {
                if self.arrow_id(a).is_err() {
                    return bad(format!("capability {} names unknown arrow `{a}`", c.name));
                }
            }
        }
        for a in &self.arrows {
            for t in &a.ensures {
                match self.capability(t) {
                    None => return bad(format!("`{}` ensures undeclared capability {t}", a.name)),
                    Some(c) if !c.issuers.contains(&a.name) => {
                        return bad(format!(
                            "unauthorized issuer of {t}: `{}` (authorized: {})",
                            a.name,
                            list(&c.issuers)
                        ));
                    }
                    _ => {}
                }
                if a.revokes.contains(t) {
                    return bad(format!("`{}` both ensures and revokes {t}", a.name));
                }
            }
            for t in &a.revokes {
                match self.capability(t) {
                    None => return bad(format!("`{}` revokes undeclared capability {t}", a.name)),
                    Some(c) if !c.revokers.contains(&a.name) => {
                        return bad(format!(
                            "unauthorized revoker of {t}: `{}` (authorized: {})",
                            a.name,
                            list(&c.revokers)
                        ));
                    }
                    _ => {}
                }
            }
        }
        for o in &self.objects {
            for t in &o.entry.needs {
                if self.capability(t).is_none() {
                    return bad(format!(
                        "entry of {} needs undeclared capability {t}",
                        o.name
                    ));
                }
            }
        }
        for o in &self.objects {
            if let Some(Join::Gate { authority, export }) = &o.join {
                let Ok(a) = self.arrow_id(authority) else {
                    return bad(format!(
                        "join at {}: unknown authority arrow `{authority}`",
                        o.name
                    ));
                };
                if !matches!(
                    self.object(self.arrow(a).src).frame.primitive,
                    Primitive::Noul | Primitive::Split
                ) {
                    return bad(format!(
                        "join at {}: authority `{authority}` must leave a noul or split frame (only those fork)",
                        o.name
                    ));
                }
                for t in export {
                    if self.capability(t).is_none() {
                        return bad(format!(
                            "join at {} exports undeclared capability {t}",
                            o.name
                        ));
                    }
                }
            }
        }
        let mut names = BTreeSet::new();
        for a in &self.attesters {
            if !names.insert(a.name.as_str()) {
                return bad(format!("attester {} is declared twice", a.name));
            }
            if a.observes.is_empty() {
                return bad(format!("attester {} observes nothing", a.name));
            }
        }
        for a in &self.arrows {
            for path in a.attested.iter().flat_map(Require::paths) {
                if !self.attesters.iter().any(|t| t.observes.contains(&path)) {
                    return bad(format!(
                        "`{}` needs attested `{path}`, but no declared attester may observe it",
                        a.name
                    ));
                }
            }
        }
        for &s in &self.starts {
            let o = self.object(s);
            if !o.entry.needs.is_empty() {
                return bad(format!(
                    "start {} needs {} on entry; a walk holding nothing cannot start there",
                    o.name,
                    o.entry.needs.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// A shortest path from `from` to `to` that never visits `avoid`, if
    /// one exists. `None` proves there is none.
    pub fn reachable(&self, from: ObjId, to: ObjId, avoid: &[ObjId]) -> Option<Path> {
        if avoid.contains(&from) || avoid.contains(&to) {
            return None;
        }
        // Breadth-first, so the witness is a shortest path. `prev` holds
        // the arrow each object was first reached by.
        let mut prev: HashMap<ObjId, ArrowId> = HashMap::new();
        let mut seen = BTreeSet::from([from]);
        let mut queue = std::collections::VecDeque::from([from]);
        'search: while let Some(o) = queue.pop_front() {
            for &a in self.out(o) {
                let next = self.arrow(a).dst;
                if avoid.contains(&next) {
                    continue;
                }
                if next == to {
                    prev.insert(to, a);
                    break 'search;
                }
                if seen.insert(next) {
                    prev.insert(next, a);
                    queue.push_back(next);
                }
            }
        }
        let mut arrows = vec![*prev.get(&to)?];
        let mut cursor = self.arrow(arrows[0]).src;
        while cursor != from {
            let a = prev[&cursor];
            arrows.push(a);
            cursor = self.arrow(a).src;
        }
        arrows.reverse();
        self.path(&arrows).ok()
    }

    /// The first invariant this graph violates, with a counter-path.
    /// Rules are skipped: they are judged by models, not proved.
    pub fn violation(&self) -> Option<(&Invariant, Path)> {
        self.invariants.iter().find_map(|inv| {
            let id = |n: &str| self.object_id(n).ok();
            let witness = match inv {
                Invariant::Via { from, to, through } => {
                    let avoid: Vec<ObjId> = through.iter().filter_map(|n| id(n)).collect();
                    self.reachable(id(from)?, id(to)?, &avoid)
                }
                Invariant::Never { from, to } => self.reachable(id(from)?, id(to)?, &[]),
                Invariant::Rule(_) => None,
            }?;
            Some((inv, witness))
        })
    }

    /// Reapplies learned arrows (from a `.learned.jsonl` layer) on top of
    /// this category, each through [`crate::supervise::structural`], the
    /// same checks the open world applied when it learned them. Arrows that
    /// no longer pass (the declared policy changed) are skipped and
    /// returned with the reason.
    ///
    /// One rule applies to learned arrows only (**progress**): a learned
    /// arrow may not close a cycle, i.e. its target may not already lead
    /// back to its source. Declared graphs may loop on purpose (a retry);
    /// learned structure only ever moves a walk forward, so it cannot trap
    /// walks in a loop no person designed.
    pub fn with_learned(
        self,
        learned: &[crate::walk::Proposal],
    ) -> (Category, Vec<(String, String)>) {
        let mut cat = self;
        let mut skipped = Vec::new();
        for p in learned {
            if let (Ok(src), Ok(dst)) = (cat.object_id(&p.src), cat.object_id(&p.dst))
                && (src == dst || cat.reachable(dst, src, &[]).is_some())
            {
                skipped.push((
                    p.arrow.clone(),
                    format!(
                        "progress: {} already leads back to {}; a learned arrow may not close a cycle",
                        p.dst, p.src
                    ),
                ));
                continue;
            }
            let (checks, _) = crate::supervise::structural(&cat, p);
            if let Some(c) = checks
                .iter()
                .find(|c| c.outcome == crate::supervise::Outcome::Fail)
            {
                skipped.push((p.arrow.clone(), format!("{}: {}", c.check, c.reason)));
                continue;
            }
            let meta = ArrowMeta {
                instructions: (!p.about.is_empty()).then(|| Value::String(p.about.clone())),
                learned: true,
                ..ArrowMeta::default()
            };
            match cat.extend(&p.arrow, &p.src, &p.dst, meta) {
                Ok(next) => {
                    let snapshot = cat.snapshot.clone();
                    cat = next;
                    cat.snapshot = snapshot;
                }
                Err(e) => skipped.push((p.arrow.clone(), e.to_string())),
            }
        }
        (cat, skipped)
    }

    /// This category plus one more arrow (and its target, if new): the
    /// graph a proposal would produce. The new object is open.
    pub fn extend(
        &self,
        arrow: &str,
        src: &str,
        dst: &str,
        meta: ArrowMeta,
    ) -> Result<Category, Error> {
        let mut b = CategoryBuilder::new(self.name.clone());
        for o in &self.objects {
            b.object(&o.name)?;
            b.frame(&o.name, o.frame.clone())?;
            if let Some(about) = &o.about {
                b.about(&o.name, about.clone())?;
            }
            if o.closure == Closure::Closed {
                b.close(&o.name)?;
            }
            if !o.entry.is_empty() {
                b.entry(&o.name, o.entry.clone())?;
            }
            if let Some(j) = &o.join {
                b.join(&o.name, j.clone())?;
            }
        }
        if self.object_id(dst).is_err() {
            b.object(dst)?;
        }
        for a in &self.arrows {
            let meta = ArrowMeta {
                instructions: a.instructions.clone(),
                level: a.level,
                require: a.require.clone(),
                attested: a.attested.clone(),
                learned: a.learned,
                ensures: a.ensures.clone(),
                revokes: a.revokes.clone(),
            };
            let (s, d) = (&self.object(a.src).name, &self.object(a.dst).name);
            b.arrow_with(&a.name, s, d, meta)?;
        }
        b.arrow_with(arrow, src, dst, meta)?;
        let spec = |p: &Path| {
            if p.is_id() {
                PathSpec::Id(self.object(p.src).name.clone())
            } else {
                PathSpec::Arrows(
                    p.arrows
                        .iter()
                        .map(|a| self.arrow(*a).name.clone())
                        .collect(),
                )
            }
        };
        for e in &self.equations {
            b.equation(spec(&e.lhs), spec(&e.rhs));
        }
        for inv in &self.invariants {
            b.invariant(inv.clone());
        }
        for c in &self.capabilities {
            b.capability(c.clone());
        }
        for a in &self.attesters {
            b.attester(a.clone());
        }
        for &s in &self.starts {
            b.start(&self.object(s).name)?;
        }
        // Not validated: the caller (the supervisor) checks invariants on
        // the result itself, so a violation is reported with its proof.
        b.build_unchecked()
    }

    pub fn object(&self, id: ObjId) -> &Object {
        &self.objects[id.0 as usize]
    }

    pub fn arrow(&self, id: ArrowId) -> &Arrow {
        &self.arrows[id.0 as usize]
    }

    pub fn object_id(&self, name: &str) -> Result<ObjId, Error> {
        self.object_index
            .get(name)
            .copied()
            .ok_or_else(|| Error::UnknownObject(name.to_owned()))
    }

    pub fn arrow_id(&self, name: &str) -> Result<ArrowId, Error> {
        self.arrow_index
            .get(name)
            .copied()
            .ok_or_else(|| Error::UnknownArrow(name.to_owned()))
    }

    /// The decision frame of `obj`: all arrows leaving it, in declaration order.
    pub fn out(&self, obj: ObjId) -> &[ArrowId] {
        let lo = self.out_offsets[obj.0 as usize] as usize;
        let hi = self.out_offsets[obj.0 as usize + 1] as usize;
        &self.out_arrows[lo..hi]
    }

    /// Whether `arrow` may be taken given the case and the walk's tokens:
    /// its own `require`, then its target's entry contract evaluated on
    /// the tokens the walk would hold after the arrow's effects.
    pub fn gate(&self, arrow: ArrowId, case: &Value, tokens: &BTreeSet<String>) -> Gate {
        let a = self.arrow(arrow);
        if a.require.as_ref().is_some_and(|r| !r.eval(case)) {
            return Gate::Require;
        }
        if let Some(att) = &a.attested {
            let (view, _, _) = crate::attest::attested_view(&self.attesters, case);
            if !att.eval(&view) {
                return Gate::Unattested;
            }
        }
        let entry = &self.object(a.dst).entry;
        let after = a.effect(tokens);
        let missing: Vec<String> = entry
            .needs
            .iter()
            .filter(|t| !after.contains(*t))
            .cloned()
            .collect();
        let require_failed = entry.require.as_ref().is_some_and(|r| !r.eval(case));
        if missing.is_empty() && !require_failed {
            Gate::Open
        } else {
            Gate::Entry {
                missing,
                require_failed,
            }
        }
    }

    /// The frame's arrows whose gate is open, in frame order.
    pub fn eligible(&self, obj: ObjId, case: &Value, tokens: &BTreeSet<String>) -> Vec<ArrowId> {
        self.out(obj)
            .iter()
            .copied()
            .filter(|a| self.gate(*a, case, tokens) == Gate::Open)
            .collect()
    }

    /// Every path from `from` to `to` that visits no object twice, skips the
    /// objects in `avoid`, and has at most `max_len` arrows. Shortest first.
    pub fn paths(&self, from: ObjId, to: ObjId, avoid: &[ObjId], max_len: usize) -> Vec<Path> {
        fn go(
            cat: &Category,
            to: ObjId,
            avoid: &[ObjId],
            max_len: usize,
            path: &mut Path,
            seen: &mut Vec<ObjId>,
            out: &mut Vec<Path>,
        ) {
            if path.dst == to && !path.is_id() {
                out.push(path.clone());
                return;
            }
            if path.arrows.len() == max_len {
                return;
            }
            for &a in cat.out(path.dst) {
                let next = cat.arrow(a).dst;
                if avoid.contains(&next) || (seen.contains(&next) && next != to) {
                    continue;
                }
                let mut longer = path.clone();
                longer
                    .push(cat, a)
                    .expect("frame arrows leave the current object");
                seen.push(next);
                go(cat, to, avoid, max_len, &mut longer, seen, out);
                seen.pop();
            }
        }
        let mut out = Vec::new();
        if avoid.contains(&from) {
            return out;
        }
        go(
            self,
            to,
            avoid,
            max_len,
            &mut Path::id(from),
            &mut vec![from],
            &mut out,
        );
        out.sort_by_key(|p| p.arrows.len());
        out
    }

    /// Composes arrows given in application order (`[f, g]` is `g ∘ f`).
    pub fn path(&self, arrows: &[ArrowId]) -> Result<Path, Error> {
        let (first, rest) = arrows.split_first().ok_or(Error::EmptyPath)?;
        let mut path = Path::id(self.arrow(*first).src);
        path.push(self, *first)?;
        for a in rest {
            path.push(self, *a)?;
        }
        Ok(path)
    }
}

/// Collects declarations, then validates and freezes them into a [`Category`].
#[derive(Default)]
pub struct CategoryBuilder {
    name: String,
    objects: Vec<Object>,
    arrows: Vec<Arrow>,
    equations: Vec<(PathSpec, PathSpec)>,
    invariants: Vec<Invariant>,
    capabilities: Vec<Capability>,
    attesters: Vec<crate::attest::Attester>,
    starts: Vec<ObjId>,
    object_index: HashMap<String, ObjId>,
    arrow_index: HashMap<String, ArrowId>,
}

/// A path written by name, resolved at build time.
#[derive(Clone, Debug)]
pub enum PathSpec {
    /// Identity on the named object.
    Id(String),
    /// Arrow names in application order (`["f", "g"]` is `g ∘ f`).
    Arrows(Vec<String>),
}

/// Names the engine reserves: path terms (`o`, `id`) and the chooser's
/// no-match option.
pub const RESERVED: &[&str] = &["o", "id", NONE_OF_THESE];

/// The option every System-1 choice carries besides the frame's arrows.
pub const NONE_OF_THESE: &str = "none_of_these";

impl CategoryBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn object(&mut self, name: &str) -> Result<ObjId, Error> {
        check_name(name)?;
        if self.object_index.contains_key(name) || self.arrow_index.contains_key(name) {
            return Err(Error::Duplicate(name.to_owned()));
        }
        let id = ObjId(self.objects.len() as u32);
        self.objects.push(Object {
            name: name.to_owned(),
            closure: Closure::Open,
            about: None,
            frame: Frame::default(),
            entry: Entry::default(),
            join: None,
        });
        self.object_index.insert(name.to_owned(), id);
        Ok(id)
    }

    pub fn arrow(&mut self, name: &str, src: &str, dst: &str) -> Result<ArrowId, Error> {
        self.arrow_with(name, src, dst, ArrowMeta::default())
    }

    pub fn arrow_with(
        &mut self,
        name: &str,
        src: &str,
        dst: &str,
        meta: ArrowMeta,
    ) -> Result<ArrowId, Error> {
        check_name(name)?;
        if self.object_index.contains_key(name) || self.arrow_index.contains_key(name) {
            return Err(Error::Duplicate(name.to_owned()));
        }
        for t in meta.ensures.iter().chain(&meta.revokes) {
            check_name(t)?;
        }
        let src = self.lookup_object(src)?;
        let dst = self.lookup_object(dst)?;
        let id = ArrowId(self.arrows.len() as u32);
        self.arrows.push(Arrow {
            name: name.to_owned(),
            src,
            dst,
            instructions: meta.instructions,
            level: meta.level,
            require: meta.require,
            attested: meta.attested,
            learned: meta.learned,
            ensures: meta.ensures,
            revokes: meta.revokes,
        });
        self.arrow_index.insert(name.to_owned(), id);
        Ok(id)
    }

    pub fn close(&mut self, object: &str) -> Result<(), Error> {
        let id = self.lookup_object(object)?;
        self.objects[id.0 as usize].closure = Closure::Closed;
        Ok(())
    }

    /// Sets how `object`'s frame is decided.
    pub fn frame(&mut self, object: &str, frame: Frame) -> Result<(), Error> {
        let id = self.lookup_object(object)?;
        self.objects[id.0 as usize].frame = frame;
        Ok(())
    }

    /// Declares `object` a join point.
    pub fn join(&mut self, object: &str, join: Join) -> Result<(), Error> {
        let id = self.lookup_object(object)?;
        self.objects[id.0 as usize].join = Some(join);
        Ok(())
    }

    /// Sets `object`'s entry contract.
    pub fn entry(&mut self, object: &str, entry: Entry) -> Result<(), Error> {
        for t in &entry.needs {
            check_name(t)?;
        }
        let id = self.lookup_object(object)?;
        self.objects[id.0 as usize].entry = entry;
        Ok(())
    }

    /// Describes what `object` means.
    pub fn about(&mut self, object: &str, about: Value) -> Result<(), Error> {
        let id = self.lookup_object(object)?;
        self.objects[id.0 as usize].about = Some(about);
        Ok(())
    }

    pub fn invariant(&mut self, invariant: Invariant) {
        self.invariants.push(invariant);
    }

    /// Declares a capability; arrows are resolved when the category is
    /// built, so declarations may come before or after the arrows.
    pub fn capability(&mut self, capability: Capability) {
        self.capabilities.push(capability);
    }

    /// Declares an attester (policy).
    pub fn attester(&mut self, attester: crate::attest::Attester) {
        self.attesters.push(attester);
    }

    /// Declares an application entry point.
    pub fn start(&mut self, object: &str) -> Result<(), Error> {
        let id = self.lookup_object(object)?;
        if !self.starts.contains(&id) {
            self.starts.push(id);
        }
        Ok(())
    }

    pub fn equation(&mut self, lhs: PathSpec, rhs: PathSpec) {
        self.equations.push((lhs, rhs));
    }

    /// Validates and freezes the category; fails if a structural invariant
    /// is violated.
    pub fn build(self) -> Result<Category, Error> {
        let cat = self.build_unchecked()?;
        cat.validate_capabilities()?;
        if let Some((inv, witness)) = cat.violation() {
            return Err(Error::InvariantViolated {
                invariant: inv.to_string(),
                witness: witness.display_typed(&cat),
            });
        }
        Ok(cat)
    }

    /// As [`build`](Self::build), without checking invariants.
    pub fn build_unchecked(self) -> Result<Category, Error> {
        let n = self.objects.len();

        // Levels belong to score frames, and a score frame's arrows each
        // hold a distinct level.
        let mut levels: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); n];
        for a in &self.arrows {
            let src = &self.objects[a.src.0 as usize];
            let bad = |msg: String| Error::Frame {
                object: src.name.clone(),
                msg,
            };
            match (src.frame.primitive, a.level) {
                (Primitive::Score, None) => {
                    return Err(bad(format!(
                        "score frame: arrow `{}` needs `level N`",
                        a.name
                    )));
                }
                (Primitive::Score, Some(l)) => {
                    if !levels[a.src.0 as usize].insert(l) {
                        return Err(bad(format!("score frame: level {l} is used twice")));
                    }
                }
                (p, Some(_)) => {
                    return Err(bad(format!(
                        "arrow `{}` has a level, but the frame is decided by {}",
                        a.name,
                        p.as_str()
                    )));
                }
                (_, None) => {}
            }
        }

        // CSR: count per source, prefix-sum, then scatter in declaration order.
        let mut out_offsets = vec![0u32; n + 1];
        for a in &self.arrows {
            out_offsets[a.src.0 as usize + 1] += 1;
        }
        for i in 0..n {
            out_offsets[i + 1] += out_offsets[i];
        }
        let mut cursor = out_offsets.clone();
        let mut out_arrows = vec![ArrowId(0); self.arrows.len()];
        for (i, a) in self.arrows.iter().enumerate() {
            let slot = &mut cursor[a.src.0 as usize];
            out_arrows[*slot as usize] = ArrowId(i as u32);
            *slot += 1;
        }

        let mut cat = Category {
            snapshot: None,
            name: self.name,
            objects: self.objects,
            arrows: self.arrows,
            equations: Vec::new(),
            invariants: self.invariants,
            capabilities: self.capabilities,
            attesters: self.attesters,
            starts: self.starts,
            out_offsets,
            out_arrows,
            object_index: self.object_index,
            arrow_index: self.arrow_index,
        };

        for (lhs, rhs) in self.equations {
            let lhs = resolve(&cat, &lhs)?;
            let rhs = resolve(&cat, &rhs)?;
            if (lhs.src, lhs.dst) != (rhs.src, rhs.dst) {
                return Err(Error::EquationNotParallel {
                    lhs: lhs.display(&cat),
                    rhs: rhs.display(&cat),
                });
            }
            cat.equations.push(Equation { lhs, rhs });
        }
        // Structural invariants must hold for the graph as written.
        for inv in &cat.invariants {
            if let Invariant::Via { from, to, through } = inv {
                for n in [from, to].into_iter().chain(through) {
                    cat.object_id(n)?;
                }
            }
        }
        Ok(cat)
    }

    fn lookup_object(&self, name: &str) -> Result<ObjId, Error> {
        self.object_index
            .get(name)
            .copied()
            .ok_or_else(|| Error::UnknownObject(name.to_owned()))
    }
}

/// Resolves a by-name path against a built category.
pub fn resolve(cat: &Category, spec: &PathSpec) -> Result<Path, Error> {
    match spec {
        PathSpec::Id(obj) => Ok(Path::id(cat.object_id(obj)?)),
        PathSpec::Arrows(names) => {
            let ids = names
                .iter()
                .map(|n| cat.arrow_id(n))
                .collect::<Result<Vec<_>, _>>()?;
            cat.path(&ids)
        }
    }
}

fn list(names: &[String]) -> String {
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}

fn check_name(name: &str) -> Result<(), Error> {
    let valid = name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !valid || RESERVED.contains(&name) {
        return Err(Error::InvalidName(name.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod learned_tests {
    use crate::walk::Proposal;

    const SRC: &str = "category C {
        objects: A, B, M;
        capability G { issuers: g; }
        g: A -> B ensures G;
        m: B -> M;
        invariant via: A -> M through B;
    }";

    fn p(arrow: &str, src: &str, dst: &str) -> Proposal {
        Proposal {
            arrow: arrow.into(),
            src: src.into(),
            dst: dst.into(),
            about: "learned".into(),
            ..Default::default()
        }
    }

    #[test]
    fn learned_layer_replays_through_the_proofs() {
        let cat = crate::parse(SRC).unwrap();
        let snapshot = cat.snapshot().map(str::to_owned);
        let forged = Proposal {
            ensures: vec!["G".into()],
            ..p("mint", "B", "New")
        };
        let (cat, skipped) = cat.with_learned(&[
            p("side", "A", "Side"),
            p("bypass", "A", "M"),
            forged,
            p("next", "Side", "B"),
            p("back", "B", "Side"),
            p("self", "M", "M"),
        ]);
        let names: Vec<&str> = skipped.iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(names, ["bypass", "mint", "back", "self"]);
        assert!(skipped[2].1.starts_with("progress"));
        let side = cat.object_id("Side").unwrap();
        let learned: Vec<&str> = cat
            .out(side)
            .iter()
            .map(|a| cat.arrow(*a))
            .filter(|a| a.learned)
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(learned, ["next"]);
        // The snapshot still names the declared policy.
        assert_eq!(cat.snapshot().map(str::to_owned), snapshot);
    }
}
