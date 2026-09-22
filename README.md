# onto

**A category engine that decision models walk.** Define objects and arrows,
declare which paths are equal, and let a fast System-1 model (such as Jev)
route through the graph. Where the graph doesn't enumerate the options well
enough, the walk escalates to a language model that can propose new structure.

> Early stage (M1). Design: [`docs/00-architecture.md`](docs/00-architecture.md).

## Try it

```sh
cargo build
onto=target/debug/onto
$onto check   examples/triage.onto
$onto ls      examples/triage.onto Request
$onto eq      examples/triage.onto build.plan implement.specify   # Equal
$onto walk    examples/triage.onto --from Request --script report,patch,ship
$onto walk    examples/triage.onto --from Request --script ask     # escalates: open frame
```

## Layout

`crates/onto-core` engine (category, path, equality, walk, parse) ·
`crates/onto-cli` the `onto` binary · `examples/` `.onto` files ·
`docs/` design (start at `00-architecture.md`).

## License

MIT, with DCO sign-off on every commit. See [CONTRIBUTING.md](CONTRIBUTING.md).
