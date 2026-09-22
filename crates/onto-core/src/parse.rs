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
//! instruction (a "string" or JSON object/array), and `require EXPR` (last,
//! since the expression runs to the end of the statement).

use serde_json::Value;

use crate::category::{ArrowMeta, Category, CategoryBuilder, Frame, PathSpec, Primitive};
use crate::error::Error;
use crate::require::Require;

pub fn parse(src: &str) -> Result<Category, Error> {
    let (name, statements) = split(src)?;
    let mut b = CategoryBuilder::new(name);
    for (line, stmt) in statements {
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
            other => {
                return Err(perr(format!(
                    "unknown primitive `{other}` (choice, noul, score)"
                )));
            }
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
            },
        );
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
        (arrow, Some((src, rest))) => {
            let rest = rest.trim_start();
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let (dst, tail) = rest.split_at(end);
            b.arrow_with(arrow, src.trim(), dst.trim(), arrow_meta(tail)?)
                .map(drop)
        }
        (key, None) => Err(perr(format!(
            "unknown declaration `{key}:` (expected objects, closed, or `name: A -> B`)"
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
        } else if let Some(r) = rest.strip_prefix("require ") {
            meta.require = Some(Require::parse(r)?);
            return Ok(meta);
        } else if rest.starts_with(['"', '{', '[']) {
            let (value, tail) = json_value(rest)?;
            meta.instructions = Some(value);
            rest = tail;
        } else {
            return Err(perr(format!(
                "unexpected `{rest}` (expected `level N`, an instruction, or `require …`)"
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
