# Contributing to onto

## License and sign-off (DCO)

Contributions are accepted under **MIT** (inbound = outbound). Every commit
must carry a Developer Certificate of Origin sign-off:

    git commit -s

which appends `Signed-off-by: Your Name <you@example.com>`, certifying
https://developercertificate.org/. No CLA.

## Before you open a PR

    cargo fmt --all
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

Design changes start in [`docs/00-architecture.md`](docs/00-architecture.md),
the single source of truth for how onto is built. Code follows the doc.
