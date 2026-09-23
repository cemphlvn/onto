//! What a model sees at a frame: `state` declarations (policy).
//!
//! A System-1 model has no memory between calls: the state sent with a
//! question is its whole context. Which parts of a case reach it is a
//! policy decision (data minimization), declared next to capabilities and
//! invariants:
//!
//! ```text
//! state { goal; case: request.purpose; observed; history: last 3; }   # default
//! state Marketing, Research { case: request.purpose; observed; }      # override
//! invariant unseen: case.person.health, case.person.email;
//! ```
//!
//! Items: `goal` (the requester's free text), `case` (the whole case) or
//! `case: a.b, c` (only these fields), `observed` (the attested view:
//! verified observations only), `history` or `history: last N` (the walk's
//! earlier judgments), `focus`, `tokens`. An override replaces the default
//! for its frames. With no declaration at all a frame sees what it always
//! did: goal, the whole case, history, focus and tokens.
//!
//! `invariant unseen: case.x, observed.y` is proved on load and for every
//! proposal: no frame's state reveals the field, or any field inside or
//! around it. The proof covers the structured case and the observed view;
//! the goal is free text and is reported, never proved (`onto laws`).

use crate::attest::Attester;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum CaseView {
    #[default]
    Hidden,
    Whole,
    Fields(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StateSpec {
    pub goal: bool,
    pub case: CaseView,
    pub observed: bool,
    /// `None`: no history; `Some(n)`: the last `n` hops (`usize::MAX`: all).
    pub history: Option<usize>,
    pub focus: bool,
    pub tokens: bool,
}

impl StateSpec {
    /// What frames see when nothing is declared (the behaviour before
    /// `state` existed).
    pub fn legacy() -> Self {
        Self {
            goal: true,
            case: CaseView::Whole,
            observed: false,
            history: Some(usize::MAX),
            focus: true,
            tokens: true,
        }
    }

    /// `goal; case: a.b; observed; history: last 3; focus; tokens;`
    pub fn parse(body: &str) -> Result<Self, String> {
        let mut spec = Self::default();
        for item in body.split(';').map(str::trim).filter(|i| !i.is_empty()) {
            let (key, value) = match item.split_once(':') {
                Some((k, v)) => (k.trim(), Some(v.trim())),
                None => (item, None),
            };
            match (key, value) {
                ("goal", None) => spec.goal = true,
                ("case", None) => spec.case = CaseView::Whole,
                ("case", Some(v)) => {
                    let fields: Vec<String> = v
                        .split(',')
                        .map(|f| f.trim().to_owned())
                        .filter(|f| !f.is_empty())
                        .collect();
                    if let Some(bad) = fields.iter().find(|f| !is_path(f)) {
                        return Err(format!("`{bad}` is not a field path (a.b.c)"));
                    }
                    spec.case = CaseView::Fields(fields);
                }
                ("observed", None) => spec.observed = true,
                ("history", None) => spec.history = Some(usize::MAX),
                ("history", Some(v)) => {
                    let n = v
                        .strip_prefix("last")
                        .and_then(|n| n.trim().parse().ok())
                        .ok_or_else(|| format!("expected `history: last N`, got `{v}`"))?;
                    spec.history = Some(n);
                }
                ("focus", None) => spec.focus = true,
                ("tokens", None) => spec.tokens = true,
                _ => {
                    return Err(format!(
                        "unknown state item `{item}` (goal, case, case: a.b, observed, history, history: last N, focus, tokens)"
                    ));
                }
            }
        }
        Ok(spec)
    }

    /// Why this state would show `path` (`case.a.b` or `observed.a.b`) to
    /// a model, if it would.
    pub fn reveals(&self, path: &str, attesters: &[Attester]) -> Option<String> {
        if let Some(field) = path.strip_prefix("case.") {
            return match &self.case {
                CaseView::Hidden => None,
                CaseView::Whole => Some("it shows the whole case".into()),
                CaseView::Fields(fs) => fs
                    .iter()
                    .find(|f| overlaps(f, field))
                    .map(|f| format!("it shows case field `{f}`")),
            };
        }
        if let Some(field) = path.strip_prefix("observed.") {
            if !self.observed {
                return None;
            }
            return attesters.iter().find_map(|a| {
                a.observes.iter().find(|o| overlaps(o, field)).map(|o| {
                    format!(
                        "it shows the observed view, where {} observes `{o}`",
                        a.name
                    )
                })
            });
        }
        None
    }

    /// One line: what this state shows.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.goal {
            parts.push("goal (free text)".to_owned());
        }
        match &self.case {
            CaseView::Hidden => {}
            CaseView::Whole => parts.push("the whole case".into()),
            CaseView::Fields(fs) => parts.push(format!("case: {}", fs.join(", "))),
        }
        if self.observed {
            parts.push("observed (attested only)".into());
        }
        match self.history {
            None => {}
            Some(usize::MAX) => parts.push("history".into()),
            Some(n) => parts.push(format!("history: last {n}")),
        }
        if self.focus {
            parts.push("focus".into());
        }
        if self.tokens {
            parts.push("tokens".into());
        }
        if parts.is_empty() {
            "only the frame itself".into()
        } else {
            parts.join(" · ")
        }
    }
}

/// A shown field reveals a protected one when either contains the other.
fn overlaps(shown: &str, protected: &str) -> bool {
    shown == protected
        || shown.starts_with(&format!("{protected}."))
        || protected.starts_with(&format!("{shown}."))
}

pub fn is_path(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reveals() {
        let s =
            StateSpec::parse("goal; case: request.purpose, person.name; history: last 2").unwrap();
        assert_eq!(s.history, Some(2));
        assert!(s.reveals("case.request", &[]).is_some()); // around a shown field
        assert!(s.reveals("case.person.name.first", &[]).is_some()); // inside
        assert!(s.reveals("case.person.health", &[]).is_none());
        assert!(
            StateSpec::legacy()
                .reveals("case.person.health", &[])
                .is_some()
        );
        assert!(StateSpec::parse("case: a..b").is_err());
        assert!(StateSpec::parse("memory").is_err());
    }
}

#[cfg(test)]
mod proofs {
    use crate::walk::Proposal;

    const SRC: &str = r#"category C {
        objects: Use, Marketing, Sent;
        state { goal; case: request.purpose; }
        state Marketing { case: request.purpose, request.channel; observed; }
        send: Use -> Marketing;
        go: Marketing -> Sent;
        invariant unseen: case.person.health, case.person;
    }"#;

    #[test]
    fn unseen_is_proved_on_load() {
        let cat = crate::parse(SRC).unwrap();
        let m = cat.object_id("Marketing").unwrap();
        assert!(cat.state_of(m).observed);
        assert!(!cat.state_of(cat.object_id("Use").unwrap()).observed);
        // A frame that shows the person is refused on load, with the frame.
        let leaky = SRC.replace(
            "state Marketing { case: request.purpose, request.channel; observed; }",
            "state Marketing { case: request.purpose, person.name; }",
        );
        let e = crate::parse(&leaky).unwrap_err().to_string();
        assert!(
            e.contains("frame Marketing would show `case.person`"),
            "{e}"
        );
        // Without a default, a frame with no declaration sees everything.
        let no_default = SRC.replace("state { goal; case: request.purpose; }", "");
        let e = crate::parse(&no_default).unwrap_err().to_string();
        assert!(
            e.contains("frame Use") && e.contains("no `state` is declared"),
            "{e}"
        );
    }

    #[test]
    fn new_objects_inherit_the_default_and_stay_proved() {
        let cat = crate::parse(SRC).unwrap();
        let p = Proposal {
            arrow: "archive".into(),
            src: "Use".into(),
            dst: "Archive".into(),
            ..Default::default()
        };
        let (checks, _) = crate::supervise::structural(&cat, &p);
        let unseen = checks
            .iter()
            .find(|c| c.subject.starts_with("unseen"))
            .unwrap();
        assert_eq!(unseen.outcome, crate::supervise::Outcome::Pass);
    }
}
