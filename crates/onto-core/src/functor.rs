//! Functors between categories (`docs/05-functors.md`).
//!
//! A functor maps the objects in its domain to objects of the target and
//! every arrow between two mapped objects to a path of the target. It is
//! defined on the full subcategory of mapped objects (a partial functor):
//! a standard need not describe a whole organization.
//!
//! Loading checks that it is well defined and that equations are kept
//! (equality saturation: `Distinct` fails, `Unknown` is reported). The
//! policy checks (authority, contracts, invariants reflected; coverage)
//! are reported, and are load errors when listed in `require:`.

use std::collections::{BTreeMap, BTreeSet};

use crate::category::{ArrowId, Category, Invariant, ObjId, PathSpec, resolve};
use crate::equality::{Equality, Verdict};
use crate::error::Error;
use crate::path::Path;

/// A functor as written: names, resolved against its categories by
/// [`Functor::build`].
#[derive(Clone, Debug, Default)]
pub struct FunctorDecl {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub objects: Vec<(String, String)>,
    pub arrows: Vec<(String, PathSpec)>,
    pub capabilities: Vec<(String, String)>,
    pub require: Vec<Obligation>,
    /// Phase 2: empty fibers become proposals when a walk escalates.
    pub transport: bool,
    /// `by name;`: objects and arrows not mapped explicitly map to the
    /// same name in the target, where it exists (versions).
    pub by_name: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Obligation {
    Authority,
    Contracts,
    Invariants,
    Cover,
}

impl Obligation {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "authority" => Ok(Self::Authority),
            "contracts" => Ok(Self::Contracts),
            "invariants" => Ok(Self::Invariants),
            "cover" => Ok(Self::Cover),
            other => Err(format!(
                "unknown obligation `{other}` (authority, contracts, invariants, cover)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authority => "authority",
            Self::Contracts => "contracts",
            Self::Invariants => "invariants",
            Self::Cover => "cover",
        }
    }
}

/// A validated functor with its report.
#[derive(Clone, Debug)]
pub struct Functor {
    pub name: String,
    pub src: String,
    pub dst: String,
    /// Per source object: its image, or `None` outside the domain.
    pub objects: Vec<Option<ObjId>>,
    /// Per source arrow: its image path, or `None` outside the domain.
    pub arrows: Vec<Option<Path>>,
    pub capabilities: BTreeMap<String, String>,
    pub require: Vec<Obligation>,
    pub transport: bool,
    /// Declared `by name` (a version): relabelling and changed frames are
    /// reported.
    pub by_name: bool,
    pub report: Report,
}

/// What a functor keeps, reflects and leaves uncovered.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Equations of the source and whether their images are equal.
    pub equations: Vec<(String, Verdict)>,
    /// Target objects and arrows with no preimage.
    pub uncovered_objects: Vec<String>,
    pub uncovered_arrows: Vec<String>,
    /// Per target arrow: the source arrows mapping onto it (as the first
    /// arrow of their image or anywhere in it), and whether each is
    /// attested.
    pub backing: Vec<(String, Vec<(String, bool)>)>,
    /// `f ensures T` but `F(f)` does not ensure `F(T)`.
    pub not_preserved: Vec<String>,
    /// Mapped onto a single thing whose description (or frame question)
    /// differs: the same structure, a changed meaning.
    pub relabelled: Vec<String>,
    /// Source objects whose frame gained or lost options in the target
    /// (a target arrow out of the image with no preimage, or the reverse):
    /// the question asked there changed.
    pub changed_frames: Vec<String>,
    pub authority: Vec<String>,
    pub contracts: Vec<String>,
    pub invariants: Vec<String>,
}

impl Report {
    pub fn violations(&self, o: Obligation) -> Vec<String> {
        match o {
            Obligation::Authority => self.authority.clone(),
            Obligation::Contracts => self.contracts.clone(),
            Obligation::Invariants => self.invariants.clone(),
            Obligation::Cover => self
                .uncovered_arrows
                .iter()
                .map(|a| format!("arrow `{a}` has no preimage"))
                .collect(),
        }
    }
}

impl Functor {
    pub fn build(d: &FunctorDecl, a: &Category, b: &Category) -> Result<Self, Error> {
        let bad = |msg: String| Error::Functor {
            functor: d.name.clone(),
            msg,
        };
        let mut objects = vec![None; a.objects().len()];
        for (x, y) in &d.objects {
            let xi = a.object_id(x).map_err(|e| bad(e.to_string()))?;
            let yi = b.object_id(y).map_err(|e| bad(e.to_string()))?;
            if objects[xi.0 as usize].replace(yi).is_some() {
                return Err(bad(format!("object `{x}` is mapped twice")));
            }
        }
        if d.by_name {
            for (i, o) in a.objects().iter().enumerate() {
                if objects[i].is_none() {
                    objects[i] = b.object_id(&o.name).ok();
                }
            }
        }
        let mut arrows: Vec<Option<Path>> = vec![None; a.arrows().len()];
        for (f, spec) in &d.arrows {
            let fi = a.arrow_id(f).map_err(|e| bad(e.to_string()))?;
            let fa = a.arrow(fi);
            let (Some(s), Some(t)) = (objects[fa.src.0 as usize], objects[fa.dst.0 as usize])
            else {
                return Err(bad(format!(
                    "arrow `{f}` is mapped, but {} or {} is outside the domain",
                    a.object(fa.src).name,
                    a.object(fa.dst).name
                )));
            };
            // `id` alone means the identity on the image of the source.
            let spec = match spec {
                PathSpec::Arrows(v) if v.len() == 1 && v[0] == "id" => {
                    PathSpec::Id(b.object(s).name.clone())
                }
                other => other.clone(),
            };
            let p = resolve(b, &spec).map_err(|e| bad(format!("`{f}`: {e}")))?;
            if (p.src, p.dst) != (s, t) {
                return Err(bad(format!(
                    "`{f}: {} -> {}` must map to a path {} -> {}, but `{}` runs {} -> {}",
                    a.object(fa.src).name,
                    a.object(fa.dst).name,
                    b.object(s).name,
                    b.object(t).name,
                    p.display(b),
                    b.object(p.src).name,
                    b.object(p.dst).name
                )));
            }
            if arrows[fi.0 as usize].replace(p).is_some() {
                return Err(bad(format!("arrow `{f}` is mapped twice")));
            }
        }
        if d.by_name {
            for (i, fa) in a.arrows().iter().enumerate() {
                let ends = (objects[fa.src.0 as usize], objects[fa.dst.0 as usize]);
                if arrows[i].is_none()
                    && let (Some(s), Some(t)) = ends
                    && let Ok(g) = b.arrow_id(&fa.name)
                    && (b.arrow(g).src, b.arrow(g).dst) == (s, t)
                {
                    arrows[i] = Some(Path {
                        src: s,
                        dst: t,
                        arrows: vec![g],
                    });
                }
            }
        }
        for (i, fa) in a.arrows().iter().enumerate() {
            let inside =
                objects[fa.src.0 as usize].is_some() && objects[fa.dst.0 as usize].is_some();
            if inside && arrows[i].is_none() {
                return Err(bad(format!(
                    "arrow `{}` runs between mapped objects ({} -> {}) but is not mapped",
                    fa.name,
                    a.object(fa.src).name,
                    a.object(fa.dst).name
                )));
            }
        }
        let mut capabilities = BTreeMap::new();
        for (t, u) in &d.capabilities {
            if !a.capabilities().iter().any(|c| &c.name == t) {
                return Err(bad(format!("`{t}` is not a capability of {}", a.name())));
            }
            if !b.capabilities().iter().any(|c| &c.name == u) {
                return Err(bad(format!("`{u}` is not a capability of {}", b.name())));
            }
            capabilities.insert(t.clone(), u.clone());
        }
        if d.by_name {
            for c in a.capabilities() {
                if !capabilities.contains_key(&c.name)
                    && b.capabilities().iter().any(|x| x.name == c.name)
                {
                    capabilities.insert(c.name.clone(), c.name.clone());
                }
            }
        }
        let mut f = Self {
            name: d.name.clone(),
            src: d.src.clone(),
            dst: d.dst.clone(),
            objects,
            arrows,
            capabilities,
            require: d.require.clone(),
            transport: d.transport,
            by_name: d.by_name,
            report: Report::default(),
        };
        f.report = f.check(a, b)?;
        for o in &f.require {
            let v = f.report.violations(*o);
            if !v.is_empty() {
                return Err(bad(format!("requires {}: {}", o.as_str(), v.join("; "))));
            }
        }
        Ok(f)
    }

    /// The image of a source path, if it stays inside the domain.
    pub fn image(&self, a: &Category, b: &Category, p: &Path) -> Option<Path> {
        let mut out = Path::id((*self.objects.get(p.src.0 as usize)?)?);
        for f in &p.arrows {
            for g in &self.arrows.get(f.0 as usize)?.as_ref()?.arrows {
                out.push(b, *g).ok()?;
            }
        }
        let _ = a;
        Some(out)
    }

    /// Source arrows leaving `x` whose image starts with `g` (the fiber
    /// over `g` at `x`).
    pub fn fiber(&self, a: &Category, x: ObjId, g: ArrowId) -> Vec<ArrowId> {
        a.out(x)
            .iter()
            .copied()
            .filter(|f| {
                self.arrows
                    .get(f.0 as usize)
                    .and_then(Option::as_ref)
                    .and_then(|p| p.arrows.first())
                    == Some(&g)
            })
            .collect()
    }

    /// Target arrows leaving `F(x)` that no arrow of `x` maps onto: what
    /// the target knows and the source lacks at `x`.
    pub fn empty_fibers(&self, a: &Category, b: &Category, x: ObjId) -> Vec<ArrowId> {
        let Some(&Some(y)) = self.objects.get(x.0 as usize) else {
            return Vec::new();
        };
        b.out(y)
            .iter()
            .copied()
            .filter(|g| self.fiber(a, x, *g).is_empty())
            .collect()
    }

    /// Source objects mapping to `y`.
    /// The image of a source object, if it is in the domain.
    pub fn object(&self, x: ObjId) -> Option<ObjId> {
        *self.objects.get(x.0 as usize)?
    }

    pub fn preimage(&self, y: ObjId) -> Vec<ObjId> {
        (0..self.objects.len() as u32)
            .map(ObjId)
            .filter(|x| self.objects[x.0 as usize] == Some(y))
            .collect()
    }

    fn check(&self, a: &Category, b: &Category) -> Result<Report, Error> {
        let mut r = Report::default();

        // Equations inside the domain must hold for the images.
        let eq = Equality::new(b)?;
        for e in a.equations() {
            let (Some(l), Some(rr)) = (self.image(a, b, &e.lhs), self.image(a, b, &e.rhs)) else {
                continue;
            };
            let verdict = eq.check(&l, &rr);
            let shown = format!("{} = {}", e.lhs.display(a), e.rhs.display(a));
            if verdict == Verdict::Distinct {
                return Err(Error::Functor {
                    functor: self.name.clone(),
                    msg: format!(
                        "equation `{shown}` is not kept: `{}` and `{}` are distinct in {}",
                        l.display(b),
                        rr.display(b),
                        b.name()
                    ),
                });
            }
            r.equations.push((shown, verdict));
        }

        // Coverage and evidence backing.
        let covered_objects: BTreeSet<ObjId> = self.objects.iter().flatten().copied().collect();
        let mut backing: BTreeMap<ArrowId, Vec<(String, bool)>> = BTreeMap::new();
        for (i, p) in self.arrows.iter().enumerate() {
            let Some(p) = p else { continue };
            let fa = &a.arrows()[i];
            for g in &p.arrows {
                backing
                    .entry(*g)
                    .or_default()
                    .push((fa.name.clone(), fa.attested.is_some()));
            }
        }
        for (i, o) in b.objects().iter().enumerate() {
            if !covered_objects.contains(&ObjId(i as u32)) {
                r.uncovered_objects.push(o.name.clone());
            }
        }
        for (i, g) in b.arrows().iter().enumerate() {
            match backing.remove(&ArrowId(i as u32)) {
                Some(v) => r.backing.push((g.name.clone(), v)),
                None => r.uncovered_arrows.push(g.name.clone()),
            }
        }

        // Versions: same structure, changed meaning; frames whose options
        // changed. (For views and standards, mapping onto differently
        // described arrows is the point, not a change.)
        let versions = self.by_name;
        for (i, p) in self.arrows.iter().enumerate().filter(|_| versions) {
            if let Some(p) = p
                && let [g] = p.arrows.as_slice()
                && a.arrows()[i].instructions != b.arrow(*g).instructions
            {
                r.relabelled.push(format!(
                    "arrow `{}` → `{}`",
                    a.arrows()[i].name,
                    b.arrow(*g).name
                ));
            }
        }
        for (i, y) in self.objects.iter().enumerate().filter(|_| versions) {
            let Some(y) = y else { continue };
            let (x, yo) = (&a.objects()[i], b.object(*y));
            if x.frame.instructions != yo.frame.instructions {
                r.relabelled
                    .push(format!("frame question of {} → {}", x.name, yo.name));
            }
            let x_id = ObjId(i as u32);
            let gained: Vec<&str> = self
                .empty_fibers(a, b, x_id)
                .iter()
                .map(|g| b.arrow(*g).name.as_str())
                .collect();
            let lost = a
                .out(x_id)
                .iter()
                .any(|f| self.arrows[f.0 as usize].is_none());
            if !gained.is_empty() || lost {
                r.changed_frames.push(if gained.is_empty() {
                    format!("{}: lost options", x.name)
                } else {
                    format!("{}: gained {}", x.name, gained.join(", "))
                });
            }
        }

        // Capabilities: preserved (report) and authority (reflected).
        for (i, p) in self.arrows.iter().enumerate() {
            let Some(p) = p else { continue };
            let fa = &a.arrows()[i];
            let issued: BTreeSet<&str> = p
                .arrows
                .iter()
                .flat_map(|g| b.arrow(*g).ensures.iter().map(String::as_str))
                .collect();
            for t in &fa.ensures {
                if let Some(u) = self.capabilities.get(t)
                    && !issued.contains(u.as_str())
                {
                    r.not_preserved.push(format!(
                        "`{}` ensures {t}, but its image `{}` does not ensure {u}",
                        fa.name,
                        p.display(b)
                    ));
                }
            }
            for u in issued {
                let sourced = fa
                    .ensures
                    .iter()
                    .any(|t| self.capabilities.get(t).map(String::as_str) == Some(u));
                if !sourced {
                    r.authority.push(format!(
                        "`{}` maps onto `{}`, which issues {u}, but `{}` ensures no capability mapped to {u}",
                        fa.name,
                        p.display(b),
                        fa.name
                    ));
                }
            }
        }

        // Entry contracts, reflected.
        for (i, y) in self.objects.iter().enumerate() {
            let Some(y) = y else { continue };
            let x = &a.objects()[i];
            for need in &b.object(*y).entry.needs {
                let ok = x
                    .entry
                    .needs
                    .iter()
                    .any(|t| self.capabilities.get(t) == Some(need));
                if !ok {
                    r.contracts.push(format!(
                        "{} needs {need} on entry, but {} needs no capability mapped to it",
                        b.object(*y).name,
                        x.name
                    ));
                }
            }
        }

        // Structural invariants of the target, reflected on preimages.
        for inv in b.invariants() {
            let pre = |n: &str| -> Vec<ObjId> {
                b.object_id(n).map(|y| self.preimage(y)).unwrap_or_default()
            };
            let (from, to, avoid) = match inv {
                Invariant::Via { from, to, through } => (
                    pre(from),
                    pre(to),
                    through.iter().flat_map(|t| pre(t)).collect::<Vec<_>>(),
                ),
                Invariant::Never { from, to } => (pre(from), pre(to), Vec::new()),
                Invariant::Rule(_) | Invariant::Unseen(_) => continue,
            };
            for p in &from {
                for q in &to {
                    if let Some(w) = a.reachable(*p, *q, &avoid) {
                        r.invariants.push(format!(
                            "{}'s `{inv}` fails here: `{}`",
                            b.name(),
                            w.display_typed(a)
                        ));
                    }
                }
            }
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use crate::equality::Verdict;

    const SRC: &str = r#"
    # an organization's process and a standard it maps into
    category Ops {
        objects: Req, Change, Tests, Verified, Board, Cleared, Deployed;
        capability Grant { issuers: approve; }
        classify: Req -> Change;
        test_line: Change -> Tests;
        tested: Tests -> Verified;
        high: Verified -> Board;
        approve: Board -> Cleared ensures Grant;
        deploy: Cleared -> Deployed;
        entry Deployed: needs Grant;
    }
    category Std {
        objects: Requested, Assessed, Verified, Authorized, Implemented;
        capability Authorized { issuers: authorize; }
        assess: Requested -> Assessed;
        verify: Assessed -> Verified;
        authorize: Verified -> Authorized ensures Authorized;
        implement: Authorized -> Implemented;
        rollback: Implemented -> Requested;
        entry Implemented: needs Authorized;
        invariant via: Requested -> Implemented through Verified;
    }
    functor Controls: Ops -> Std {
        objects: Req -> Requested, Change -> Assessed, Tests -> Assessed,
                 Verified -> Verified, Board -> Verified, Cleared -> Authorized, Deployed -> Implemented;
        capabilities: Grant -> Authorized;
        classify: assess; test_line: id; tested: verify; high: id;
        approve: authorize; deploy: implement;
        require: authority, contracts, invariants;
    }
    "#;

    fn module(src: &str) -> Result<crate::Module, crate::Error> {
        crate::parse_module(src, &mut |p| Err(crate::Error::UnknownObject(p.into())))
    }

    #[test]
    fn a_standard_is_reflected() {
        let m = module(SRC).unwrap();
        let f = m.functor("Controls").unwrap();
        assert_eq!(f.report.uncovered_arrows, ["rollback"]);
        let verify = f
            .report
            .backing
            .iter()
            .find(|(g, _)| g == "verify")
            .unwrap();
        assert_eq!(verify.1, [("tested".to_owned(), false)]);
        // The image of a walk is a path of the standard.
        let (a, b) = (m.category("Ops").unwrap(), m.category("Std").unwrap());
        let ids: Vec<_> = [
            "classify",
            "test_line",
            "tested",
            "high",
            "approve",
            "deploy",
        ]
        .iter()
        .map(|n| a.arrow_id(n).unwrap())
        .collect();
        let p = a.path(&ids).unwrap();
        assert_eq!(
            f.image(a, b, &p).unwrap().display(b),
            "implement.authorize.verify.assess"
        );
    }

    #[test]
    fn missing_contract_or_bypass_is_refused_when_required() {
        let no_contract = SRC.replace("        entry Deployed: needs Grant;\n", "");
        let e = module(&no_contract).unwrap_err().to_string();
        assert!(e.contains("requires contracts"), "{e}");
        // Mapping a Change -> Board arrow onto `verify` declares it a
        // verification: the invariant holds. Hiding verification inside a
        // single arrow straight to Cleared does not count: invariants are
        // reflected on objects.
        let declared = SRC
            .replace(
                "deploy: Cleared -> Deployed;",
                "deploy: Cleared -> Deployed;\n        fast: Change -> Board;",
            )
            .replace("high: id;", "high: id; fast: verify;");
        assert!(module(&declared).is_ok());
        let bypass = SRC
            .replace(
                "deploy: Cleared -> Deployed;",
                "deploy: Cleared -> Deployed;\n        rush: Change -> Cleared;",
            )
            .replace("high: id;", "high: id; rush: authorize.verify;");
        // Caught twice: it also maps onto an arrow that issues authority
        // without having any.
        let e = module(&bypass).unwrap_err().to_string();
        assert!(e.contains("requires authority"), "{e}");
        let only_invariants = bypass.replace(
            "require: authority, contracts, invariants;",
            "require: invariants;",
        );
        let e = module(&only_invariants).unwrap_err().to_string();
        assert!(
            e.contains("requires invariants") && e.contains("rush"),
            "{e}"
        );
    }

    #[test]
    fn functor_must_be_well_defined_and_keep_equations() {
        let wrong_end = SRC.replace("tested: verify;", "tested: assess;");
        assert!(
            module(&wrong_end)
                .unwrap_err()
                .to_string()
                .contains("must map to a path")
        );
        let unmapped = SRC.replace("high: id;", "");
        assert!(
            module(&unmapped)
                .unwrap_err()
                .to_string()
                .contains("not mapped")
        );
        let eq = r#"
        category A { objects: X, Y; f: X -> Y; g: X -> Y; f = g; }
        category B { objects: P, Q; h: P -> Q; k: P -> Q; }
        functor F: A -> B { objects: X -> P, Y -> Q; f: h; g: k; }
        "#;
        assert!(module(eq).unwrap_err().to_string().contains("is not kept"));
        let kept = eq.replace("g: k;", "g: h;");
        let m = module(&kept).unwrap();
        assert_eq!(m.functors[0].report.equations[0].1, Verdict::Equal);
    }

    #[test]
    fn a_version_functor_reports_what_changed() {
        let src = r#"
        category V1 { objects: C, D, Done; frame C: choice "Which?";
            d: C -> D "own disability"; none: C -> Done "nothing"; go: D -> Done; }
        category V2 { objects: C, D, Care, Done; frame C: choice "Which, about themselves?";
            d: C -> D "own disability"; care: C -> Care "cares for someone"; none: C -> Done "nothing special";
            go: D -> Done; cared: Care -> Done; }
        functor Up: V1 -> V2 { by name; }
        "#;
        let m = module(src).unwrap();
        let r = &m.functor("Up").unwrap().report;
        assert_eq!(r.changed_frames, ["C: gained care"]);
        assert_eq!(r.uncovered_arrows, ["care", "cared"]);
        assert!(r.relabelled.iter().any(|x| x.contains("`none`")));
        assert!(
            r.relabelled
                .iter()
                .any(|x| x.contains("frame question of C"))
        );
        assert!(!r.relabelled.iter().any(|x| x.contains("`d`")));
    }
}
