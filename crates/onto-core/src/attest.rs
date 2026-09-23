//! Attested observations: signed statements about the world by declared
//! attesters.
//!
//! A case may carry observations:
//!
//! ```json
//! "observations": [{"claim": {"errors.stopped": true}, "attester": "DeployBot",
//!                   "case": "INC-4411", "at": "2026-09-23T10:02:00Z",
//!                   "signature": "<base64 Ed25519>"}]
//! ```
//!
//! An observation counts only if (1) its attester is declared in the
//! policy, (2) the signature verifies (strictly) under that attester's key
//! over [`message`], (3) the attester may observe every claimed field, and
//! (4) it is bound to this case (`case` equals the case's `id`), so it
//! cannot be replayed into another case. The **attested view** is the
//! merge of the claims of all such observations; `attested P` preconditions
//! are evaluated against it, never against plain case facts.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::{Map, Value, json};

/// A declared source of observations: its public key and the fields it may
/// observe. Part of the policy (the root of trust).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attester {
    pub name: String,
    pub key: [u8; 32],
    pub observes: Vec<String>,
}

impl Attester {
    /// `ed25519:<base64 of the 32-byte public key>`
    pub fn parse_key(s: &str) -> Result<[u8; 32], String> {
        let b64 = s
            .strip_prefix("ed25519:")
            .ok_or("keys are written `ed25519:<base64>`")?;
        let bytes = B64
            .decode(b64.trim())
            .map_err(|e| format!("key is not base64: {e}"))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "an Ed25519 public key is 32 bytes".to_owned())?;
        VerifyingKey::from_bytes(&key).map_err(|e| format!("not a valid Ed25519 key: {e}"))?;
        Ok(key)
    }

    pub fn key_string(&self) -> String {
        format!("ed25519:{}", B64.encode(self.key))
    }
}

/// The bytes an attester signs: a domain tag, then the canonical JSON
/// (sorted keys) of attester, case, time and claim.
pub fn message(attester: &str, case: &str, at: &str, claim: &Map<String, Value>) -> Vec<u8> {
    let body = json!({"attester": attester, "case": case, "at": at, "claim": claim});
    let mut out = b"onto-observation-v1\n".to_vec();
    out.extend(
        serde_json::to_string(&body)
            .expect("serializable")
            .as_bytes(),
    );
    out
}

/// One observation that passed every check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    pub attester: String,
    pub fields: Vec<String>,
    pub at: String,
}

/// Why an observation was not counted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    pub attester: String,
    pub reason: String,
}

/// The attested view of a case: claims of every valid observation, merged
/// into nested JSON (`errors.stopped` becomes `{"errors": {"stopped": …}}`),
/// plus which observations counted and which did not.
pub fn attested_view(
    attesters: &[Attester],
    case: &Value,
) -> (Value, Vec<Verified>, Vec<Rejected>) {
    let mut view = Value::Object(Map::new());
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    let case_id = case["id"].as_str();
    for o in case["observations"].as_array().into_iter().flatten() {
        let name = o["attester"].as_str().unwrap_or_default().to_owned();
        let reject = |reason: String| Rejected {
            attester: name.clone(),
            reason,
        };
        let Some(att) = attesters.iter().find(|a| a.name == name) else {
            bad.push(reject("not a declared attester".into()));
            continue;
        };
        let (Some(claim), Some(at), Some(bound)) =
            (o["claim"].as_object(), o["at"].as_str(), o["case"].as_str())
        else {
            bad.push(reject(
                "malformed observation (needs claim, at, case, signature)".into(),
            ));
            continue;
        };
        if Some(bound) != case_id {
            bad.push(reject(format!("bound to case `{bound}`, not this case")));
            continue;
        }
        let sig = o["signature"]
            .as_str()
            .and_then(|s| B64.decode(s).ok())
            .and_then(|b| <[u8; 64]>::try_from(b).ok())
            .map(|b| Signature::from_bytes(&b));
        let key = VerifyingKey::from_bytes(&att.key).expect("validated at load");
        let verified = sig.is_some_and(|s| {
            key.verify_strict(&message(&name, bound, at, claim), &s)
                .is_ok()
        });
        if !verified {
            bad.push(reject("signature does not verify".into()));
            continue;
        };
        if let Some(field) = claim.keys().find(|f| !att.observes.contains(f)) {
            bad.push(reject(format!("not authorized to observe `{field}`")));
            continue;
        }
        for (field, value) in claim {
            insert(&mut view, field, value.clone());
        }
        ok.push(Verified {
            attester: name.clone(),
            fields: claim.keys().cloned().collect(),
            at: at.to_owned(),
        });
    }
    (view, ok, bad)
}

fn insert(view: &mut Value, dotted: &str, value: Value) {
    let mut cursor = view;
    let parts: Vec<&str> = dotted.split('.').collect();
    for (i, p) in parts.iter().enumerate() {
        let map = cursor.as_object_mut().expect("objects all the way down");
        if i + 1 == parts.len() {
            map.insert((*p).to_owned(), value);
            return;
        }
        cursor = map
            .entry((*p).to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if !cursor.is_object() {
            *cursor = Value::Object(Map::new());
        }
    }
}
