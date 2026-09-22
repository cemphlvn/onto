//! The `.onto` text format.
//!
//! ```text
//! # comments run to end of line
//! category Triage {
//!     objects: Request, Question, Bug;
//!     ask:    Request -> Question;
//!     report: Request -> Bug;
//!     closed: Request;          # Request's outgoing arrows are MECE
//!     g.f = h;                  # path equation (g after f equals h)
//!     inv.f = id(A);            # identities are written id(Object)
//! }
//! ```
//!
//! Statements end with `;`. Objects must be declared before use.

use crate::category::{Category, CategoryBuilder, PathSpec};
use crate::error::Error;

pub fn parse(src: &str) -> Result<Category, Error> {
    // Blank out comments but keep newlines, so byte offsets map to lines.
    let text: String = src
        .lines()
        .map(|l| l.split_once('#').map_or(l, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n");
    let line_of = |offset: usize| text[..offset].matches('\n').count() + 1;

    let open = text.find('{').ok_or(Error::Parse {
        line: 1,
        msg: "expected `category Name {`".into(),
    })?;
    let close = text.rfind('}').ok_or(Error::Parse {
        line: line_of(text.len()),
        msg: "missing closing `}`".into(),
    })?;
    let header: Vec<&str> = text[..open].split_whitespace().collect();
    let name = match header.as_slice() {
        ["category" | "cat", name] => *name,
        _ => {
            return Err(Error::Parse {
                line: line_of(open),
                msg: "expected `category Name {`".into(),
            });
        }
    };
    if !text[close + 1..].trim().is_empty() {
        return Err(Error::Parse {
            line: line_of(close),
            msg: "unexpected text after `}`".into(),
        });
    }

    let mut b = CategoryBuilder::new(name);
    let mut offset = open + 1;
    for stmt in text[open + 1..close].split(';') {
        let start = offset + (stmt.len() - stmt.trim_start().len());
        offset += stmt.len() + 1;
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        statement(&mut b, stmt).map_err(|e| match e {
            Error::Parse { msg, .. } => Error::Parse {
                line: line_of(start),
                msg,
            },
            other => Error::Parse {
                line: line_of(start),
                msg: other.to_string(),
            },
        })?;
    }
    b.build()
}

fn statement(b: &mut CategoryBuilder, stmt: &str) -> Result<(), Error> {
    if let Some((lhs, rhs)) = stmt.split_once('=') {
        b.equation(path_spec(lhs)?, path_spec(rhs)?);
        return Ok(());
    }
    let (key, value) = stmt.split_once(':').ok_or_else(|| Error::Parse {
        line: 0,
        msg: format!("cannot read `{stmt}`"),
    })?;
    match (key.trim(), value.split_once("->")) {
        ("objects", None) => list(value).try_for_each(|o| b.object(o).map(drop)),
        ("closed", None) => list(value).try_for_each(|o| b.close(o)),
        (arrow, Some((src, dst))) => b.arrow(arrow, src.trim(), dst.trim()).map(drop),
        (key, None) => Err(Error::Parse {
            line: 0,
            msg: format!(
                "unknown declaration `{key}:` (expected objects, closed, or `name: A -> B`)"
            ),
        }),
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
        return Err(Error::Parse {
            line: 0,
            msg: format!("malformed path `{s}`"),
        });
    }
    names.reverse();
    Ok(PathSpec::Arrows(names))
}
