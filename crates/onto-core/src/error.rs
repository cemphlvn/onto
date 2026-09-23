use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown object `{0}`")]
    UnknownObject(String),
    #[error("unknown arrow `{0}`")]
    UnknownArrow(String),
    #[error("`{0}` is declared twice")]
    Duplicate(String),
    #[error(
        "`{0}` is not a valid name (letters, digits, `_`; `o`, `id` and `none_of_these` are reserved)"
    )]
    InvalidName(String),
    #[error("empty path; write `id(Object)` for an identity")]
    EmptyPath,
    #[error(
        "cannot compose `{arrow}` after `{path}`: expected an arrow from `{expected}`, found one from `{found}`"
    )]
    NotComposable {
        path: String,
        arrow: String,
        expected: String,
        found: String,
    },
    #[error("equation `{lhs} = {rhs}` relates paths with different endpoints")]
    EquationNotParallel { lhs: String, rhs: String },
    #[error("line {line}: {msg}")]
    Parse { line: usize, msg: String },
    #[error("frame `{object}`: {msg}")]
    Frame { object: String, msg: String },
    #[error("invariant `{invariant}` is violated by `{witness}`")]
    InvariantViolated { invariant: String, witness: String },
    #[error("capability: {0}")]
    Capability(String),
    #[error("functor {functor}: {msg}")]
    Functor { functor: String, msg: String },
    #[error("require {0}")]
    Require(String),
    #[error("rewrite construction failed: {0}")]
    Rewrite(String),
}
