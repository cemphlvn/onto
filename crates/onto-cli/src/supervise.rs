//! `onto review` and `onto promote`: the supervisor from the command line.
//!
//! `review` checks every provisional proposal in a dispositions file and
//! writes one verdict per line. `promote` is the human step: it takes one
//! reviewed proposal, re-proves its structural checks against the current
//! file, and writes it into the `.onto` file with a provenance comment.
//! Git is the delta log.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use onto_core::supervise::{Admission, Outcome, structural};
use onto_core::walk::Proposal;
use onto_runtime::model::{Critic, MockCritic, ModelError, NoulQuestion, Usage};
use onto_runtime::providers::Jev;
use onto_runtime::supervisor::{Review, review};
use serde_json::Value;

use crate::run::BoxError;

#[derive(Args)]
pub struct ReviewArgs {
    /// The category the proposals would extend.
    file: PathBuf,
    /// A dispositions file from `onto run --dispositions`.
    dispositions: PathBuf,
    /// Write the reviews here (JSON lines).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Use the offline mock critic instead of Jev.
    #[arg(long)]
    mock: bool,
}

#[derive(Args)]
pub struct PromoteArgs {
    /// The category file to extend.
    file: PathBuf,
    /// A reviews file from `onto review --out`.
    reviews: PathBuf,
    /// The review to promote (e.g. `r3`).
    id: String,
    /// Promote an `unknown` verdict after looking at it yourself.
    #[arg(long)]
    override_unknown: bool,
}

enum AnyCritic {
    Jev(Jev),
    Mock(MockCritic),
}

impl Critic for AnyCritic {
    fn name(&self) -> String {
        match self {
            Self::Jev(c) => c.name(),
            Self::Mock(c) => c.name(),
        }
    }
    async fn nouls(
        &self,
        state: Value,
        qs: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        match self {
            Self::Jev(c) => c.nouls(state, qs).await,
            Self::Mock(c) => c.nouls(state, qs).await,
        }
    }
}

pub fn review_main(args: ReviewArgs) -> Result<(), BoxError> {
    let src =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let cat = onto_core::parse(&src)?;

    // Collect proposals in record order; merge identical ones.
    let text = std::fs::read_to_string(&args.dispositions)
        .map_err(|e| format!("{}: {e}", args.dispositions.display()))?;
    let mut merged: Vec<(Proposal, Vec<String>)> = Vec::new();
    let mut index: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let record: Value = serde_json::from_str(line)?;
        let source = record["id"].as_str().unwrap_or("?").to_owned();
        for p in record["proposals"].as_array().into_iter().flatten() {
            let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
            let proposal = Proposal {
                arrow: s("arrow"),
                src: s("src"),
                dst: s("dst"),
                about: s("about"),
                rationale: s("rationale"),
                ensures: names(&p["ensures"]),
                revokes: names(&p["revokes"]),
            };
            let key = (
                proposal.src.clone(),
                proposal.arrow.clone(),
                proposal.dst.clone(),
            );
            match index.get(&key) {
                Some(&i) => merged[i].1.push(source.clone()),
                None => {
                    index.insert(key, merged.len());
                    merged.push((proposal, vec![source.clone()]));
                }
            }
        }
    }
    let current = cat.snapshot().unwrap_or_default().to_owned();
    let other_snapshots: BTreeMap<String, ()> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|r| r["snapshot"].as_str().map(str::to_owned))
        .filter(|h| *h != current)
        .map(|h| (h, ()))
        .collect();
    for h in other_snapshots.keys() {
        println!(
            "note: some proposals were made against snapshot {}; reviewing them against the current file, snapshot {}",
            short(h),
            short(&current)
        );
    }
    if merged.is_empty() {
        println!(
            "no provisional proposals in {}",
            args.dispositions.display()
        );
        return Ok(());
    }

    let critic = if args.mock {
        AnyCritic::Mock(MockCritic)
    } else {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        AnyCritic::Jev(
            Jev::from_env(http, None).ok_or("TYPESAFE_API_KEY is not set (or pass --mock)")?,
        )
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let proposals: Vec<Proposal> = merged.iter().map(|(p, _)| p.clone()).collect();
    let reviews: Vec<Review> = rt.block_on(async {
        // Every review runs concurrently; each sees the proposals before it
        // in the batch as siblings.
        let futures = merged.iter().enumerate().map(|(i, (p, sources))| {
            review(
                &cat,
                format!("r{}", i + 1),
                sources.clone(),
                p,
                &proposals[..i],
                &critic,
            )
        });
        futures_join(futures).await
    });

    for r in &reviews {
        print_review(r);
    }
    let count = |a| reviews.iter().filter(|r| r.admission == a).count();
    println!(
        "\n{} reviewed: {} admit · {} reject · {} unknown",
        reviews.len(),
        count(Admission::Admit),
        count(Admission::Reject),
        count(Admission::Unknown)
    );
    if let Some(out) = &args.out {
        let lines: Vec<String> = reviews
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<_, _>>()?;
        std::fs::write(out, lines.join("\n") + "\n")?;
        println!(
            "reviews: {}  (promote one with `onto promote {} {} <id>`)",
            out.display(),
            args.file.display(),
            out.display()
        );
    }
    Ok(())
}

/// Awaits all futures concurrently, keeping order.
async fn futures_join<F: std::future::Future>(futures: impl Iterator<Item = F>) -> Vec<F::Output> {
    let mut pinned: Vec<_> = futures.map(Box::pin).collect();
    let mut out: Vec<Option<F::Output>> = (0..pinned.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (i, f) in pinned.iter_mut().enumerate() {
            if out[i].is_none() {
                match f.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => out[i] = Some(v),
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    out.into_iter()
        .map(|v| v.expect("all futures completed"))
        .collect()
}

fn print_review(r: &Review) {
    let verdict = match r.admission {
        Admission::Admit => "ADMIT  ",
        Admission::Reject => "REJECT ",
        Admission::Unknown => "UNKNOWN",
    };
    let p = &r.proposal;
    println!(
        "{}  {verdict}  {}: {} -> {}   (from {})",
        r.id,
        p.arrow,
        p.src,
        p.dst,
        r.sources.join(", ")
    );
    for c in &r.checks {
        if c.outcome == Outcome::Pass {
            continue;
        }
        let mark = if c.outcome == Outcome::Fail {
            "✗"
        } else {
            "?"
        };
        let witness = c
            .witness
            .as_ref()
            .map_or(String::new(), |w| format!("  [counter-path: {w}]"));
        println!(
            "      {mark} {} · {}: {}{witness}",
            c.check, c.subject, c.reason
        );
    }
}

pub fn promote_main(args: PromoteArgs) -> Result<(), BoxError> {
    let text = std::fs::read_to_string(&args.reviews)
        .map_err(|e| format!("{}: {e}", args.reviews.display()))?;
    let review: Value = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|r| r["id"] == args.id.as_str())
        .ok_or_else(|| format!("no review `{}` in {}", args.id, args.reviews.display()))?;
    match review["admission"].as_str() {
        Some("admit") => {}
        Some("unknown") if args.override_unknown => {}
        Some("unknown") => {
            return Err(format!(
                "{} is unknown; look at its checks, then pass --override-unknown to promote it",
                args.id
            )
            .into());
        }
        Some(other) => {
            return Err(format!(
                "{} was {other}ed; rejected proposals cannot be promoted",
                args.id
            )
            .into());
        }
        None => return Err("review has no admission".into()),
    }
    let s = |k: &str| {
        review["proposal"][k]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let p = Proposal {
        arrow: s("arrow"),
        src: s("src"),
        dst: s("dst"),
        about: s("about"),
        rationale: s("rationale"),
        ensures: names(&review["proposal"]["ensures"]),
        revokes: names(&review["proposal"]["revokes"]),
    };

    // Serialize promotions of this file, then check, under the lock, that
    // the file is still the snapshot the review judged: a changed file
    // means the semantic checks may be stale, and only a re-review fixes it.
    let _lock = FileLock::acquire(&args.file)?;
    let original =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let current = onto_core::parse::snapshot_hash(&original);
    match review["snapshot"].as_str() {
        Some(h) if h == current => {}
        Some(h) => {
            return Err(format!(
                "{} was reviewed against snapshot {}, but {} is now {}: its checks may be stale; re-run `onto review`",
                args.id,
                short(h),
                args.file.display(),
                short(&current)
            )
            .into());
        }
        None => {
            return Err(format!("{} records no snapshot; re-run `onto review`", args.id).into());
        }
    }
    // Defense in depth: prove the structure again on the current file.
    let cat = onto_core::parse(&original)?;
    let (checks, _) = structural(&cat, &p);
    if let Some(c) = checks.iter().find(|c| c.outcome == Outcome::Fail) {
        return Err(format!(
            "no longer valid against {}: {} · {}: {}",
            args.file.display(),
            c.check,
            c.subject,
            c.reason
        )
        .into());
    }

    let sources = review["sources"].as_array().map_or(String::new(), |a| {
        a.iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    });
    let mut block = format!(
        "\n    # promoted {} UTC · review {} of snapshot {} · {}{} · from {sources}\n",
        today(),
        args.id,
        short(&current),
        review["admission"].as_str().unwrap_or("?"),
        if review["admission"] == "unknown" {
            " (overridden by a person)"
        } else {
            ""
        },
    );
    if cat.object_id(&p.dst).is_err() {
        block.push_str(&format!("    objects: {};\n", p.dst));
    }
    let about = if p.about.is_empty() {
        String::new()
    } else {
        format!(" {}", serde_json::to_string(&p.about)?)
    };
    let mut effects = String::new();
    if !p.ensures.is_empty() {
        effects.push_str(&format!(" ensures {}", p.ensures.join(", ")));
    }
    if !p.revokes.is_empty() {
        effects.push_str(&format!(" revokes {}", p.revokes.join(", ")));
    }
    block.push_str(&format!(
        "    {}: {} -> {}{about}{effects};\n",
        p.arrow, p.src, p.dst
    ));

    let close = original
        .rfind('}')
        .ok_or("category file has no closing `}`")?;
    let updated = format!(
        "{}{block}{}",
        original[..close].trim_end_matches([' ', '\t']),
        &original[close..]
    );
    onto_core::parse(&updated)
        .map_err(|e| format!("promotion would not load, file unchanged: {e}"))?;
    // Compare-and-swap: the file must not have changed since we read it;
    // then replace it atomically (write a sibling, rename over).
    let now = std::fs::read_to_string(&args.file)?;
    if onto_core::parse::snapshot_hash(&now) != current {
        return Err(format!(
            "{} changed during promotion; nothing written",
            args.file.display()
        )
        .into());
    }
    let tmp = args.file.with_extension("onto.promoting");
    std::fs::write(&tmp, &updated)?;
    std::fs::rename(&tmp, &args.file)?;
    println!(
        "promoted {}: {} -> {} into {}",
        p.arrow,
        p.src,
        p.dst,
        args.file.display()
    );
    println!(
        "snapshot {} → {}. Review the diff and commit it: git is the delta log.",
        short(&current),
        short(&onto_core::parse::snapshot_hash(&updated))
    );
    Ok(())
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

/// An exclusive lock on promotions of one file: `<file>.lock`, created
/// atomically, removed on drop.
struct FileLock(PathBuf);

impl FileLock {
    fn acquire(file: &std::path::Path) -> Result<Self, BoxError> {
        let path = file.with_extension("onto.lock");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write;
                let _ = writeln!(f, "{}", std::process::id());
                Ok(Self(path))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
                "another promotion holds {}; if none is running, remove it",
                path.display()
            )
            .into()),
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// Today's date (UTC) as YYYY-MM-DD, without a date library.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Howard Hinnant's days-to-civil algorithm.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
