//! `onto keygen` and `onto attest`: produce keys and signed observations.
//!
//! These stand in for the systems that would attest in production (a
//! deploy pipeline, a key-management service). The secret key never goes
//! into a `.onto` file; only the public key does, in an `attester` block.

use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use clap::Args;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Map, Value, json};

use crate::run::BoxError;

#[derive(Args)]
pub struct KeygenArgs {
    /// Where to write the secret key (base64). Created with mode 0600.
    out: PathBuf,
}

#[derive(Args)]
pub struct AttestArgs {
    /// Secret key file from `onto keygen`.
    #[arg(long)]
    key: PathBuf,
    /// The attester's declared name.
    #[arg(long)]
    attester: String,
    /// The case this observation is bound to (its `id`).
    #[arg(long)]
    case: String,
    /// Claims as `field=value` (value parsed as JSON if possible).
    #[arg(required = true)]
    claims: Vec<String>,
}

pub fn keygen(args: KeygenArgs) -> Result<(), BoxError> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| format!("no OS randomness: {e}"))?;
    let key = SigningKey::from_bytes(&secret);
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args.out)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        writeln!(f, "{}", B64.encode(secret))?;
    }
    println!(
        "secret key: {} (keep it out of the repository)",
        args.out.display()
    );
    println!(
        "public key: ed25519:{}",
        B64.encode(key.verifying_key().to_bytes())
    );
    Ok(())
}

pub fn attest(args: AttestArgs) -> Result<(), BoxError> {
    let secret = B64.decode(std::fs::read_to_string(&args.key)?.trim())?;
    let secret: [u8; 32] = secret.try_into().map_err(|_| "a secret key is 32 bytes")?;
    let key = SigningKey::from_bytes(&secret);
    let mut claim = Map::new();
    for c in &args.claims {
        let (field, value) = c
            .split_once('=')
            .ok_or_else(|| format!("expected field=value, got `{c}`"))?;
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
        claim.insert(field.to_owned(), value);
    }
    let at = now_utc();
    let msg = onto_core::attest::message(&args.attester, &args.case, &at, &claim);
    let signature = B64.encode(key.sign(&msg).to_bytes());
    let obs = json!({"claim": claim, "attester": args.attester, "case": args.case, "at": at, "signature": signature});
    println!("{}", serde_json::to_string(&obs)?);
    Ok(())
}

/// RFC 3339 UTC timestamp, without a date library.
fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
