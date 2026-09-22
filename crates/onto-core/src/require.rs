//! Structured preconditions: `require` clauses checked in code against a
//! walk's JSON state before any model is asked.
//!
//! ```text
//! require consent.marketing == true and age >= 16
//! require charges.duplicate            # bare path: must be truthy
//! ```
//!
//! Paths are dot-separated field names. Operators: `== != < <= > >=`.
//! Literals: numbers, "strings", true, false, null. A missing path makes
//! its clause false, so an arrow never opens on absent evidence.

use std::fmt;

use serde_json::Value;

use crate::error::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct Require {
    source: String,
    clauses: Vec<Clause>,
}

#[derive(Clone, Debug, PartialEq)]
struct Clause {
    path: Vec<String>,
    test: Option<(Op, Value)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Require {
    pub fn parse(source: &str) -> Result<Self, Error> {
        let bad = |msg: &str| Error::Require(format!("`{source}`: {msg}"));
        let clauses = source
            .split(" and ")
            .map(|clause| {
                let clause = clause.trim();
                let ops = [
                    ("==", Op::Eq),
                    ("!=", Op::Ne),
                    ("<=", Op::Le),
                    (">=", Op::Ge),
                    ("<", Op::Lt),
                    (">", Op::Gt),
                ];
                let (path, test) = match ops
                    .iter()
                    .find_map(|(s, op)| clause.split_once(s).map(|(l, r)| (l, *op, r)))
                {
                    Some((lhs, op, rhs)) => {
                        let lit: Value = serde_json::from_str(rhs.trim()).map_err(|_| {
                            bad(&format!(
                                "`{}` is not a literal (number, \"string\", true, false, null)",
                                rhs.trim()
                            ))
                        })?;
                        (lhs.trim(), Some((op, lit)))
                    }
                    None => (clause, None),
                };
                let path: Vec<String> = path.split('.').map(|s| s.trim().to_owned()).collect();
                if path
                    .iter()
                    .any(|s| s.is_empty() || !s.chars().all(|c| c.is_alphanumeric() || c == '_'))
                {
                    return Err(bad(&format!("`{}` is not a field path", path.join("."))));
                }
                Ok(Clause { path, test })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            source: source.trim().to_owned(),
            clauses,
        })
    }

    /// True when every clause holds in `state`.
    pub fn eval(&self, state: &Value) -> bool {
        self.clauses.iter().all(|c| c.eval(state))
    }
}

impl Clause {
    fn eval(&self, state: &Value) -> bool {
        let Some(v) = self.path.iter().try_fold(state, |v, key| v.get(key)) else {
            return false;
        };
        let Some((op, lit)) = &self.test else {
            return truthy(v);
        };
        match (op, v.as_f64().zip(lit.as_f64())) {
            (Op::Eq, _) => v == lit,
            (Op::Ne, _) => v != lit,
            (op, Some((a, b))) => match op {
                Op::Lt => a < b,
                Op::Le => a <= b,
                Op::Gt => a > b,
                Op::Ge => a >= b,
                Op::Eq | Op::Ne => unreachable!(),
            },
            // Ordering between non-numbers is undefined: the clause fails.
            _ => false,
        }
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

impl fmt::Display for Require {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}
