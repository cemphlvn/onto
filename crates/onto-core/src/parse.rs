//! The `.onto` text format.
//!
//! ```text
//! # comments run to end of line (outside strings)
//! category Triage {
//!     objects: Request, Question, Bug;
//!     about Bug: "something that used to work no longer does";
//!
//!     frame Request: choice "Which kind of request is this?";
//!     ask:    Request -> Question "the person wants information";
//!     report: Request -> Bug      {"meaning": "a defect", "examples": ["crash", "500 error"]};
//!     closed: Request;          # Request's outgoing arrows are MECE
//!
//!     frame Alert: noul;        # each arrow's condition judged on its own
//!     frame Impact: score "How badly are users affected?";
//!     watch: Impact -> Watch level 0 "no user-visible impact";
//!
//!     market: Consented -> Marketing require consent.marketing == true;
//!
//!     g.f = h;                  # path equation (g after f equals h)
//!     inv.f = id(A);            # identities are written id(Object)
//! }
//! ```
//!
//! Statements end with `;`. Objects must be declared before use. Arrow
//! declarations take, in any order after `Src -> Dst`: `level N`, an
//! instruction (a "string" or JSON object/array), `require EXPR`,
//! `ensures T, …` and `revokes T, …`. A `require` expression runs until an
//! `ensures`/`revokes` keyword or the end of the statement.
//! `entry Object: needs T, … require EXPR;` states an entry contract.

use serde_json::Value;

use crate::category::{
    ArrowMeta, Capability, Category, CategoryBuilder, Entry, Frame, Invariant, Join, PathSpec,
    Primitive,
};
use crate::error::Error;
use crate::require::Require;

/// Several categories and the functors between them: one file and what
/// it imports.
#[derive(Clone, Debug, Default)]
pub struct Module {
    pub categories: Vec<Category>,
    pub functors: Vec<crate::functor::Functor>,
}

impl Module {
    pub fn category(&self, name: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.name() == name)
    }

    pub fn functor(&self, name: &str) -> Option<&crate::functor::Functor> {
        self.functors.iter().find(|f| f.name == name)
    }
}

enum Block {
    Import(String),
    Category(String),
    Functor(String),
}

/// Top-level blocks of a file, with the line each starts on: `import
/// "path";`, `category Name { … }`, `functor F: A -> B { … }`.
fn blocks(src: &str) -> Result<Vec<(usize, Block)>, Error> {
    let mut out = Vec::new();
    let mut current = String::new();
    let (mut line, mut start, mut depth) = (1usize, 1usize, 0usize);
    let (mut in_str, mut escaped, mut in_comment) = (false, false, false);
    for c in src.chars() {
        if c == '\n' {
            line += 1;
            in_comment = false;
        }
        if in_comment {
            continue;
        }
        if current.trim().is_empty() && !c.is_whitespace() && c != '#' {
            start = line;
        }
        if in_str {
            current.push(c);
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '#' => {
                in_comment = true;
                continue;
            }
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth = depth.checked_sub(1).ok_or(Error::Parse {
                    line,
                    msg: format!("unbalanced `{c}`"),
                })?;
            }
            _ => {}
        }
        current.push(c);
        let text = current.trim();
        if depth == 0 && c == '}' {
            let block = if text.starts_with("category ") || text.starts_with("cat ") {
                Block::Category(text.to_owned())
            } else if let Some(rest) = text.strip_prefix("functor ") {
                Block::Functor(rest.to_owned())
            } else {
                return Err(Error::Parse {
                    line: start,
                    msg: "expected `category Name { … }` or `functor F: A -> B { … }`".into(),
                });
            };
            out.push((start, block));
            current.clear();
        } else if depth == 0 && c == ';' {
            let path = text
                .strip_prefix("import")
                .map(|r| r.trim_end_matches(';').trim())
                .and_then(|r| r.strip_prefix('"')?.strip_suffix('"'))
                .ok_or(Error::Parse {
                    line: start,
                    msg: format!("expected `import \"path\";`, got `{text}`"),
                })?;
            out.push((start, Block::Import(path.to_owned())));
            current.clear();
        }
    }
    if depth != 0 {
        return Err(Error::Parse {
            line,
            msg: "missing closing `}`".into(),
        });
    }
    if !current.trim().is_empty() {
        return Err(Error::Parse {
            line: start,
            msg: format!("unexpected `{}`", current.trim()),
        });
    }
    Ok(out)
}

/// Parses a file that may hold several categories, functors between them,
/// and imports. `import` returns the source of an imported path (the
/// caller resolves paths and refuses cycles). Categories take the
/// snapshot of the file they are written in.
pub fn parse_module(
    src: &str,
    import: &mut dyn FnMut(&str) -> Result<String, Error>,
) -> Result<Module, Error> {
    parse_module_at("", src, &mut |path, _| Ok((path.to_owned(), import(path)?)))
}

/// As [`parse_module`], for sources with an identity (a file path):
/// `import(path, importer)` resolves `path` as written in `importer` and
/// returns the imported source's identity and text.
/// Resolves `(path, importer)` to the imported source's identity and text.
pub type Importer<'a> = dyn FnMut(&str, &str) -> Result<(String, String), Error> + 'a;

pub fn parse_module_at(id: &str, src: &str, import: &mut Importer) -> Result<Module, Error> {
    let mut module = Module::default();
    let mut decls = Vec::new();
    for (line, block) in blocks(src)? {
        match block {
            Block::Import(path) => {
                let (child, text) = import(&path, id)?;
                let m = parse_module_at(&child, &text, import).map_err(|e| Error::Parse {
                    line,
                    msg: format!("in import \"{path}\": {e}"),
                })?;
                module.categories.extend(m.categories);
                module.functors.extend(m.functors);
            }
            Block::Category(text) => {
                let mut cat = category(&text, line)?;
                cat.set_snapshot(snapshot_hash(src));
                if module.category(cat.name()).is_some() {
                    return Err(Error::Parse {
                        line,
                        msg: format!("category `{}` is declared twice", cat.name()),
                    });
                }
                module.categories.push(cat);
            }
            Block::Functor(text) => decls.push((
                line,
                functor_decl(&text).map_err(|e| match e {
                    Error::Parse { msg, .. } => Error::Parse { line, msg },
                    other => other,
                })?,
            )),
        }
    }
    for (line, d) in decls {
        let (Some(a), Some(b)) = (module.category(&d.src), module.category(&d.dst)) else {
            return Err(Error::Parse {
                line,
                msg: format!(
                    "functor {}: categories `{}` and `{}` must both be declared or imported",
                    d.name, d.src, d.dst
                ),
            });
        };
        let f = crate::functor::Functor::build(&d, a, b)?;
        module.functors.push(f);
    }
    Ok(module)
}

/// `F: A -> B { objects: X -> Y, …; capabilities: T -> U; f: g.h; require: …; transport; }`
fn functor_decl(text: &str) -> Result<crate::functor::FunctorDecl, Error> {
    let (head, body) = text
        .split_once('{')
        .ok_or_else(|| perr("expected `functor F: A -> B { … }`"))?;
    let (name, ends) = head
        .split_once(':')
        .ok_or_else(|| perr("expected `functor F: A -> B { … }`"))?;
    let (src, dst) = ends
        .split_once("->")
        .ok_or_else(|| perr("expected `functor F: A -> B { … }`"))?;
    let body = body
        .trim()
        .strip_suffix('}')
        .ok_or_else(|| perr("missing `}`"))?;
    let mut d = crate::functor::FunctorDecl {
        name: name.trim().to_owned(),
        src: src.trim().to_owned(),
        dst: dst.trim().to_owned(),
        ..Default::default()
    };
    let pairs = |v: &str| -> Result<Vec<(String, String)>, Error> {
        list(v)
            .map(|p| {
                p.split_once("->")
                    .map(|(x, y)| (x.trim().to_owned(), y.trim().to_owned()))
                    .ok_or_else(|| perr(format!("expected `X -> Y`, got `{p}`")))
            })
            .collect()
    };
    for item in body.split(';').map(str::trim).filter(|i| !i.is_empty()) {
        if item == "transport" {
            d.transport = true;
            continue;
        }
        if item == "by name" {
            d.by_name = true;
            continue;
        }
        let (key, value) = item
            .split_once(':')
            .ok_or_else(|| perr(format!("cannot read `{item}`")))?;
        match key.trim() {
            "objects" => d.objects.extend(pairs(value)?),
            "capabilities" => d.capabilities.extend(pairs(value)?),
            "require" => {
                for o in list(value) {
                    d.require
                        .push(crate::functor::Obligation::parse(o).map_err(perr)?);
                }
            }
            arrow => d.arrows.push((arrow.to_owned(), path_spec(value)?)),
        }
    }
    Ok(d)
}

pub fn parse(src: &str) -> Result<Category, Error> {
    let blocks = blocks(src)?;
    if blocks.len() != 1 || !matches!(blocks[0].1, Block::Category(_)) {
        return Err(Error::Parse {
            line: 1,
            msg: "this file holds imports, functors or several categories; load it as a module"
                .into(),
        });
    }
    let mut cat = category(src, 1)?;
    cat.set_snapshot(snapshot_hash(src));
    Ok(cat)
}

/// One `category Name { … }` block starting at `line0`.
fn category(src: &str, line0: usize) -> Result<Category, Error> {
    let (name, statements) = split(src).map_err(|e| shift(e, line0))?;
    let mut b = CategoryBuilder::new(name);
    for (line, stmt) in statements {
        let line = line + line0 - 1;
        statement(&mut b, &stmt).map_err(|e| match e {
            Error::Parse { msg, .. } => Error::Parse { line, msg },
            other => Error::Parse {
                line,
                msg: other.to_string(),
            },
        })?;
    }
    b.build()
}

fn shift(e: Error, line0: usize) -> Error {
    match e {
        Error::Parse { line, msg } => Error::Parse {
            line: line + line0 - 1,
            msg,
        },
        other => other,
    }
}

/// SHA-256 of a category's source text, as lowercase hex: the identity of
/// the snapshot that walks, reviews and promotions refer to.
pub fn snapshot_hash(src: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(src.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn perr(msg: impl Into<String>) -> Error {
    Error::Parse {
        line: 0,
        msg: msg.into(),
    }
}

/// Strips comments and splits the body into `(line, statement)` pairs,
/// honouring strings and JSON nesting.
fn split(src: &str) -> Result<(String, Vec<(usize, String)>), Error> {
    let mut header = String::new();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut start_line = 1;
    let (mut line, mut depth) = (1usize, 0usize);
    let (mut in_str, mut escaped, mut in_comment, mut closed) = (false, false, false, false);

    for c in src.chars() {
        if c == '\n' {
            line += 1;
            in_comment = false;
        }
        if in_comment {
            continue;
        }
        if closed {
            if !c.is_whitespace() {
                if c == '#' {
                    in_comment = true;
                    continue;
                }
                return Err(Error::Parse {
                    line,
                    msg: "unexpected text after `}`".into(),
                });
            }
            continue;
        }
        if in_str {
            current.push(c);
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '#' => in_comment = true,
            '"' if depth >= 1 => {
                in_str = true;
                current.push(c);
            }
            '{' | '[' => {
                depth += 1;
                if depth > 1 {
                    current.push(c);
                }
            }
            '}' | ']' => {
                depth = depth.checked_sub(1).ok_or(Error::Parse {
                    line,
                    msg: format!("unbalanced `{c}`"),
                })?;
                if depth == 0 {
                    closed = true;
                } else {
                    current.push(c);
                    // A `capability Name { … }` block is a whole statement;
                    // its trailing `;` is optional.
                    if depth == 1
                        && c == '}'
                        && (current.trim_start().starts_with("capability ")
                            || current.trim_start().starts_with("attester ")
                            || current.trim_start().starts_with("state ")
                            || current.trim_start().starts_with("state{"))
                    {
                        statements.push((start_line, current.trim().to_owned()));
                        current.clear();
                    }
                }
            }
            ';' if depth == 1 => {
                if !current.trim().is_empty() {
                    statements.push((start_line, current.trim().to_owned()));
                }
                current.clear();
            }
            _ if depth == 0 => header.push(c),
            _ => {
                if current.trim().is_empty() && !c.is_whitespace() {
                    start_line = line;
                }
                current.push(c);
            }
        }
    }
    if !closed {
        return Err(Error::Parse {
            line,
            msg: "missing closing `}`".into(),
        });
    }
    if !current.trim().is_empty() {
        statements.push((start_line, current.trim().to_owned()));
    }
    let words: Vec<&str> = header.split_whitespace().collect();
    let name = match words.as_slice() {
        ["category" | "cat", name] => (*name).to_owned(),
        _ => {
            return Err(Error::Parse {
                line: 1,
                msg: "expected `category Name {`".into(),
            });
        }
    };
    Ok((name, statements))
}

fn statement(b: &mut CategoryBuilder, stmt: &str) -> Result<(), Error> {
    if let Some(rest) = stmt.strip_prefix("frame ") {
        let (object, rest) = rest
            .split_once(':')
            .ok_or_else(|| perr("expected `frame Object: choice|noul|score`"))?;
        let rest = rest.trim();
        let (word, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let primitive = match word {
            "choice" => Primitive::Choice,
            "noul" => Primitive::Noul,
            "score" => Primitive::Score,
            "split" => Primitive::Split,
            other => {
                return Err(perr(format!(
                    "unknown primitive `{other}` (choice, noul, score, split)"
                )));
            }
        };
        let (parallel, tail) = match tail.trim_start().strip_prefix("parallel") {
            Some(t) if primitive == Primitive::Noul => (true, t),
            Some(_) => return Err(perr("only noul frames can be `parallel`")),
            None => (false, tail),
        };
        let instructions = match tail.trim() {
            "" => None,
            t => Some(json_value(t)?.0),
        };
        return b.frame(
            object.trim(),
            Frame {
                primitive,
                instructions,
                parallel,
            },
        );
    }
    if let Some(rest) = stmt.strip_prefix("join ") {
        let (object, rest) = rest.split_once(':').ok_or_else(|| {
            perr("expected `join Object: all | race | gate authority A export T, …`")
        })?;
        let rest = rest.trim();
        let join = if rest == "all" {
            Join::All
        } else if rest == "race" {
            Join::Race
        } else if let Some(r) = rest.strip_prefix("gate authority ") {
            let (authority, tail) = names(r);
            let [authority] = authority.as_slice() else {
                return Err(perr("gate names exactly one authority arrow"));
            };
            let export = match tail.trim().strip_prefix("export ") {
                Some(e) => names(e).0,
                None if tail.trim().is_empty() => Vec::new(),
                None => return Err(perr(format!("unexpected `{}` in gate join", tail.trim()))),
            };
            Join::Gate {
                authority: authority.clone(),
                export,
            }
        } else {
            return Err(perr(format!(
                "unknown join `{rest}` (all, race, gate authority A export T)"
            )));
        };
        return b.join(object.trim(), join);
    }
    if let Some(rest) = stmt.strip_prefix("state")
        && (rest.starts_with(' ') || rest.starts_with('{'))
    {
        let (targets, body) = rest
            .split_once('{')
            .ok_or_else(|| perr("expected `state { … }` or `state A, B { … }`"))?;
        let body = body
            .trim()
            .strip_suffix('}')
            .ok_or_else(|| perr("state: missing `}`"))?;
        let spec = crate::state::StateSpec::parse(body).map_err(|e| perr(format!("state: {e}")))?;
        let targets: Vec<&str> = list(targets).collect();
        if targets.is_empty() {
            return b.state(None, spec);
        }
        for t in targets {
            b.state(Some(t), spec.clone())?;
        }
        return Ok(());
    }
    if let Some(rest) = stmt.strip_prefix("attester ") {
        b.attester(attester(rest)?);
        return Ok(());
    }
    if let Some(rest) = stmt.strip_prefix("capability ") {
        b.capability(capability(rest)?);
        return Ok(());
    }
    if let Some(rest) = stmt.strip_prefix("entry ") {
        let (object, rest) = rest
            .split_once(':')
            .ok_or_else(|| perr("expected `entry Object: needs T, … require …`"))?;
        let mut entry = Entry::default();
        let mut rest = rest.trim();
        if let Some(r) = rest.strip_prefix("needs ") {
            let (names, tail) = names(r);
            entry.needs = names;
            rest = tail.trim();
        }
        if let Some(r) = rest.strip_prefix("require ") {
            entry.require = Some(Require::parse(r)?);
        } else if !rest.is_empty() {
            return Err(perr(format!("unexpected `{rest}` in entry contract")));
        }
        if entry.is_empty() {
            return Err(perr(
                "an entry contract needs `needs T, …` and/or `require …`",
            ));
        }
        return b.entry(object.trim(), entry);
    }
    if let Some(rest) = stmt.strip_prefix("invariant ") {
        b.invariant(invariant(rest.trim())?);
        return Ok(());
    }
    if let Some(rest) = stmt.strip_prefix("about ") {
        let (object, rest) = rest
            .split_once(':')
            .ok_or_else(|| perr("expected `about Object: \"text\"`"))?;
        let (value, tail) = json_value(rest.trim())?;
        if !tail.trim().is_empty() {
            return Err(perr(format!("unexpected `{}`", tail.trim())));
        }
        return b.about(object.trim(), value);
    }
    let Some((key, value)) = stmt.split_once(':') else {
        // No `:` at all: a path equation.
        let (lhs, rhs) = stmt
            .split_once('=')
            .ok_or_else(|| perr(format!("cannot read `{stmt}`")))?;
        b.equation(path_spec(lhs)?, path_spec(rhs)?);
        return Ok(());
    };
    match (key.trim(), value.split_once("->")) {
        ("objects", None) => list(value).try_for_each(|o| b.object(o).map(drop)),
        ("closed", None) => list(value).try_for_each(|o| b.close(o)),
        ("start", None) => list(value).try_for_each(|o| b.start(o)),
        ("sealed", None) => list(value).try_for_each(|o| b.learnable(o, false)),
        ("learnable", None) => list(value).try_for_each(|o| b.learnable(o, true)),
        ("world", None) => match value.trim() {
            "open" => {
                b.closed_world(false);
                Ok(())
            }
            "closed" => {
                b.closed_world(true);
                Ok(())
            }
            other => Err(perr(format!(
                "world: expected open or closed, got `{other}`"
            ))),
        },
        (arrow, Some((src, rest))) => {
            let rest = rest.trim_start();
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let (dst, tail) = rest.split_at(end);
            b.arrow_with(arrow, src.trim(), dst.trim(), arrow_meta(tail)?)
                .map(drop)
        }
        (key, None) => Err(perr(format!(
            "unknown declaration `{key}:` (expected objects, closed, start, sealed, learnable, world, or `name: A -> B`)"
        ))),
    }
}

/// `level N`, an instruction, and `require EXPR`, after `Src -> Dst`.
fn arrow_meta(mut rest: &str) -> Result<ArrowMeta, Error> {
    let mut meta = ArrowMeta::default();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Ok(meta);
        }
        if let Some(r) = rest.strip_prefix("level ") {
            let r = r.trim_start();
            let end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
            let level = r[..end]
                .parse()
                .map_err(|_| perr("`level` needs a whole number"))?;
            meta.level = Some(level);
            rest = &r[end..];
        } else if let Some(r) = rest.strip_prefix("attested ") {
            let end = [" ensures ", " revokes ", " require "]
                .iter()
                .filter_map(|k| r.find(k))
                .min()
                .unwrap_or(r.len());
            meta.attested = Some(Require::parse(&r[..end])?);
            rest = &r[end..];
        } else if let Some(r) = rest.strip_prefix("ensures ") {
            let (names, tail) = names(r);
            meta.ensures.extend(names);
            rest = tail;
        } else if let Some(r) = rest.strip_prefix("revokes ") {
            let (names, tail) = names(r);
            meta.revokes.extend(names);
            rest = tail;
        } else if let Some(r) = rest.strip_prefix("require ") {
            // `require P ensures T` reads as a pre/postcondition: the
            // expression ends at an effect keyword.
            let end = [" ensures ", " revokes ", " attested "]
                .iter()
                .filter_map(|k| r.find(k))
                .min()
                .unwrap_or(r.len());
            meta.require = Some(Require::parse(&r[..end])?);
            rest = &r[end..];
        } else if rest.starts_with(['"', '{', '[']) {
            let (value, tail) = json_value(rest)?;
            meta.instructions = Some(value);
            rest = tail;
        } else {
            return Err(perr(format!(
                "unexpected `{rest}` (expected `level N`, an instruction, `ensures T`, `revokes T`, or `require …`)"
            )));
        }
    }
}

/// Reads one JSON value (string, object or array) from the start of `s`;
/// returns it and the remaining text.
fn json_value(s: &str) -> Result<(Value, &str), Error> {
    let mut stream = serde_json::Deserializer::from_str(s).into_iter::<Value>();
    match stream.next() {
        Some(Ok(v @ (Value::String(_) | Value::Object(_) | Value::Array(_)))) => {
            Ok((v, &s[stream.byte_offset()..]))
        }
        Some(Ok(_)) => Err(perr(
            "instructions must be a \"string\", {object} or [array]",
        )),
        Some(Err(e)) => Err(perr(format!("invalid instruction: {e}"))),
        None => Err(perr("missing instruction")),
    }
}

/// `via: A -> B through C`, `never: A -> B`, or `rule "text"` / `rule {json}`.
fn invariant(s: &str) -> Result<Invariant, Error> {
    let bad = || {
        perr(format!(
            "cannot read invariant `{s}` (via: A -> B through C · never: A -> B · rule \"…\")"
        ))
    };
    if let Some(rest) = s.strip_prefix("rule") {
        let (value, tail) = json_value(rest.trim())?;
        if !tail.trim().is_empty() {
            return Err(bad());
        }
        return Ok(Invariant::Rule(value));
    }
    let (kind, rest) = s.split_once(':').ok_or_else(bad)?;
    if kind.trim() == "unseen" {
        let fields: Vec<String> = rest
            .split(',')
            .map(|f| f.trim().to_owned())
            .filter(|f| !f.is_empty())
            .collect();
        if let Some(f) = fields.iter().find(|f| {
            !(f.starts_with("case.") || f.starts_with("observed.")) || !crate::state::is_path(f)
        }) {
            return Err(perr(format!(
                "unseen: `{f}` must be a field path under `case.` or `observed.`"
            )));
        }
        if fields.is_empty() {
            return Err(bad());
        }
        return Ok(Invariant::Unseen(fields));
    }
    let (from, rest) = rest.split_once("->").ok_or_else(bad)?;
    let from = from.trim().to_owned();
    match kind.trim() {
        "never" => Ok(Invariant::Never {
            from,
            to: rest.trim().to_owned(),
        }),
        "via" => {
            let (to, through) = rest.split_once(" through ").ok_or_else(bad)?;
            Ok(Invariant::Via {
                from,
                to: to.trim().to_owned(),
                through: through.split('|').map(|t| t.trim().to_owned()).collect(),
            })
        }
        _ => Err(bad()),
    }
}

/// `Name { key: ed25519:<base64>; observes: a.b, c; }`
fn attester(s: &str) -> Result<crate::attest::Attester, Error> {
    let bad = |msg: &str| perr(format!("attester `{}`: {msg}", s.trim()));
    let (name, body) = s
        .split_once('{')
        .ok_or_else(|| bad("expected `attester Name { key: …; observes: …; }`"))?;
    let body = body
        .trim()
        .strip_suffix('}')
        .ok_or_else(|| bad("missing `}`"))?;
    let (mut key, mut observes) = (None, Vec::new());
    for part in body.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (k, v) = part
            .split_once(':')
            .ok_or_else(|| bad("expected `key: …` or `observes: …`"))?;
        match k.trim() {
            "key" => key = Some(crate::attest::Attester::parse_key(v.trim()).map_err(|e| bad(&e))?),
            "observes" => observes.extend(
                v.split(',')
                    .map(|x| x.trim().to_owned())
                    .filter(|x| !x.is_empty()),
            ),
            other => return Err(bad(&format!("unknown field `{other}` (key, observes)"))),
        }
    }
    Ok(crate::attest::Attester {
        name: name.trim().to_owned(),
        key: key.ok_or_else(|| bad("missing `key`"))?,
        observes,
    })
}

/// `Name { issuers: a, b; revokers: c; }`
fn capability(s: &str) -> Result<Capability, Error> {
    let bad = |msg: &str| perr(format!("capability `{}`: {msg}", s.trim()));
    let (name, body) = s
        .split_once('{')
        .ok_or_else(|| bad("expected `capability Name { issuers: …; revokers: …; }`"))?;
    let body = body
        .trim()
        .strip_suffix('}')
        .ok_or_else(|| bad("missing `}`"))?;
    let mut cap = Capability {
        name: name.trim().to_owned(),
        ..Capability::default()
    };
    for part in body.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, value) = part
            .split_once(':')
            .ok_or_else(|| bad("expected `issuers: …` or `revokers: …`"))?;
        let names: Vec<String> = list(value).map(str::to_owned).collect();
        match key.trim() {
            "issuers" => cap.issuers.extend(names),
            "revokers" => cap.revokers.extend(names),
            other => return Err(bad(&format!("unknown field `{other}` (issuers, revokers)"))),
        }
    }
    Ok(cap)
}

/// A comma-separated list of names (`A, B, C`); returns the names and
/// the text after the last one.
fn names(s: &str) -> (Vec<String>, &str) {
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        let t = rest.trim_start();
        let end = t
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(t.len());
        if end == 0 {
            return (out, rest);
        }
        out.push(t[..end].to_owned());
        rest = &t[end..];
        match rest.trim_start().strip_prefix(',') {
            Some(after) => rest = after,
            None => return (out, rest),
        }
    }
}

fn list(s: &str) -> impl Iterator<Item = &str> {
    s.split(',').map(str::trim).filter(|x| !x.is_empty())
}

/// `h.g.f` (h after g after f) or `id(A)`.
pub fn path_spec(s: &str) -> Result<PathSpec, Error> {
    let s = s.trim();
    if let Some(obj) = s.strip_prefix("id(").and_then(|r| r.strip_suffix(')')) {
        return Ok(PathSpec::Id(obj.trim().to_owned()));
    }
    let mut names: Vec<String> = s.split('.').map(|n| n.trim().to_owned()).collect();
    if names.iter().any(String::is_empty) {
        return Err(perr(format!("malformed path `{s}`")));
    }
    names.reverse();
    Ok(PathSpec::Arrows(names))
}
