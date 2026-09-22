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
}

impl Primitive {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::Noul => "noul",
            Self::Score => "score",
        }
    }
}

/// How an object's frame is decided: the primitive, and optionally the
/// question put to the model (text or structured JSON).
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub primitive: Primitive,
    pub instructions: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Object {
    pub name: String,
    pub closure: Closure,
    /// What this object means (text or structured JSON).
    pub about: Option<Value>,
    pub frame: Frame,
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
}

/// Optional meaning attached to an arrow at declaration.
#[derive(Clone, Debug, Default)]
pub struct ArrowMeta {
    pub instructions: Option<Value>,
    pub level: Option<u32>,
    pub require: Option<Require>,
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
    objects: Vec<Object>,
    arrows: Vec<Arrow>,
    equations: Vec<Equation>,
    out_offsets: Vec<u32>,
    out_arrows: Vec<ArrowId>,
    object_index: HashMap<String, ObjId>,
    arrow_index: HashMap<String, ArrowId>,
}

impl Category {
    pub fn name(&self) -> &str {
        &self.name
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

    /// The frame's arrows whose `require` holds in `state`, in frame order.
    pub fn eligible(&self, obj: ObjId, state: &Value) -> Vec<ArrowId> {
        self.out(obj)
            .iter()
            .copied()
            .filter(|a| {
                self.arrow(*a)
                    .require
                    .as_ref()
                    .is_none_or(|r| r.eval(state))
            })
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

    /// Describes what `object` means.
    pub fn about(&mut self, object: &str, about: Value) -> Result<(), Error> {
        let id = self.lookup_object(object)?;
        self.objects[id.0 as usize].about = Some(about);
        Ok(())
    }

    pub fn equation(&mut self, lhs: PathSpec, rhs: PathSpec) {
        self.equations.push((lhs, rhs));
    }

    pub fn build(self) -> Result<Category, Error> {
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
            name: self.name,
            objects: self.objects,
            arrows: self.arrows,
            equations: Vec::new(),
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
