# onto — On Every Platform, With Local Models

Status: **design, agreed direction — 2026-09-25** (plan step F, beside
M4). Step F1 is built; the rest is not. Code follows this document.

## 1. What this layer is

onto as an engine that runs **inside an app on a phone, in a browser, or
on a desktop**, with its System-1 judge running **on the device**: no
token cost, no case data leaving the device, no network needed. The same
engine can still call remote models (Jev, OpenRouter) in parallel when an
app chooses to.

It extends `docs/10-bindings.md`, not replaces it: the same hourglass,
the same rules (mechanism, not policy; Rust as the single source of
truth; bindings only convert), with more clients at the top and the
model layer split so that a client can take the local half, the remote
half, or both.

```
   iOS / macOS app    Android app    PWA / web    Python    Rust callers
        │ Swift           │ Kotlin       │ JS/TS      │
   ─────┴─────────────────┴──────────────┴────────────┴──────────────────
     onto-ffi (UniFFI)            onto-wasm         onto-py (docs/10)
   ────────────────────────── FFI boundaries ────────────────────────────
     onto-runtime   concurrent engine; executor: native (tokio) | wasm
        │ holds any Judge / Proposer / Critic (type-erased)
        ▼
     onto-models    the model contract: traits, requests, answers, errors
        ▲                         ▲
     onto-local                onto-remote
     OpenJev method over       Jev + OpenRouter over HTTP; parallel,
     llama.cpp (GGUF);         retried, bounded by judge_concurrency
     batcher: one prefix,
     many option readouts
   ──────────────────────────────────────────────────────────────────────
     onto-core      categories, walks, proofs (sync; builds for wasm32)
```

## 2. The crates

| crate | holds | depends on | builds for |
|---|---|---|---|
| `onto-core` | categories, parsing, walks, proofs | egg, serde_json, sha2, ed25519 | native, wasm32 |
| `onto-models` | `Judge`, `Proposer`, `Critic`; `FrameRequest`, `ProposalRequest`, `Usage`, `ModelError`; type-erased `DynJudge` / `DynProposer` / `DynCritic` next (`docs/10` §4 item 1) | onto-core, serde | native, wasm32 |
| `onto-remote` | Jev (judge, critic) and OpenRouter (proposer) clients: retries with backoff, one POST per request | onto-models, reqwest, tokio (time) | native (wasm later: reqwest's fetch backend) |
| `onto-local` | the OpenJev judge (§4) and its batcher, behind a `Backend` trait: `llama.cpp` native, `wllama` through `onto-wasm` | onto-models | native, wasm32 |
| `onto-runtime` | the engine, joins, frames, supervisor, trace; mocks | onto-core, onto-models, tokio (sync, macros; rt + time natively); wasm-bindgen-futures, gloo-timers, web-time on wasm32 | native, wasm32 |
| `onto-ffi` | UniFFI surface: load, run, events, protocols as callback interfaces | onto-runtime, onto-local, onto-remote | iOS, Android, desktop |
| `onto-wasm` | wasm-bindgen surface; a JS judge (wllama) as a protocol object | onto-runtime, onto-local | browsers |

`onto-runtime` re-exports `onto_models` as `onto_runtime::model`, so
callers written against the old path keep working.

## 3. Rules

1. **The model contract has no engine and no network.** A provider
   (local, remote, a binding's callback) depends on `onto-models` only.
2. **The engine has no HTTP client.** An app with only local models
   ships no network code and no keys.
3. **The engine has no executor of its own.** It uses `tokio::sync`
   (runtime-free) and asks the `rt` module to spawn, sleep, read the
   clock (`Instant`: `web-time` on wasm, where `std`'s panics) and collect
   tasks: tokio natively, the JavaScript event loop on `wasm32`, chosen by
   target, not by feature. Browser futures that are not `Send` cross the
   model interfaces in `rt::SingleThread`, sound only on single-threaded
   wasm (a build with `atomics` does not compile it). On one thread
   the engine is still concurrent: walks interleave while a model call
   is pending.
4. **One model file everywhere.** Local judges read GGUF through
   llama.cpp: natively on iOS (Metal), Android (Vulkan / CPU) and
   desktop, and as `wllama` (llama.cpp in wasm) in browsers. The same
   file, the same state, the same options give comparable results across
   devices; only the hardware differs.
5. **Calibration is per model.** Thresholds tuned on Jev (the unknown
   band 0.3–0.7 of D21, `low_confidence`) are not assumed for a local
   model. Each local model is measured on the demos' labelled cases
   before an app relies on its probabilities, and its measured accuracy
   is shown beside its speed.

## 4. The local judge (`onto-local`)

The OpenJev method (SemIf): the frame's state and question are one
prompt; the options are labelled; **one forward pass**, then the logits
of the option labels are read and normalized with a softmax. No token is
sampled. A `choice` frame is one readout over its labels, a `noul` one
readout over yes/no per question, a `score` one readout over levels.

Parallelism on one device comes from **shared prefixes, not model
copies**: one model is loaded; copies would contend for the same GPU.

- The **batcher** collects the frame requests pending within a short
  window. Requests with the same state prefix (the several questions of
  a noul frame, or a critic's questions) are evaluated as one prefix
  followed by one suffix per question.
- Different cases are evaluated one prefix after another on the loaded
  model; the engine keeps walking other cases meanwhile.
- `judge_concurrency` bounds requests in flight to a remote API; for a
  local judge the batcher's window and size are the bounds instead.

Reference (SemIf's README, RTX 3090, Qwen3.5-4B): 2.33 decisions/s
scored fresh, 10.75 with serial prefix reuse, 20.03 with parallel
suffixes. No mobile measurement exists; step F3 produces one per device.

Model classes (from openjev.com): Qwen3 0.6B (639 MB) for phones and the
browser, MiniCPM5 2B (1.56 GB), Qwen3.5 4B (3.01 GB) native only on
devices with enough memory. Browsers cap a tab's memory well below the
device's, so the PWA defaults to the small models.

## 5. Plan

| step | builds | test |
|---|---|---|
| F1 ✓ | `onto-models` (the contract) and `onto-remote` (providers) out of `onto-runtime`; the runtime has no reqwest | the whole workspace test suite and the CLI unchanged |
| F2 ✓ | the `rt` module; `onto-runtime` builds and runs on `wasm32-unknown-unknown` | `tests/portable.rs`: triage and the 22 support-commons tickets give the same paths on tokio and on a JavaScript event loop (Node, `wasm-bindgen-test`); a browser run follows with F4 |
| F3 | `onto-local`: the OpenJev judge over llama.cpp, the batcher; `onto run --judge local:<gguf>` | support-commons (22 labelled tickets) on an M4 Mac: accuracy and p50/p95 beside Jev's 1.4 s |
| F4 | `onto-wasm` + wllama; the first PWA (support-commons) with a benchmark screen | the same 22 tickets on iPhone 15 Pro – 18 Pro in Safari: accuracy, p50/p95, cases/s, memory |
| F5 | `onto-ffi` (UniFFI), iOS and Android shells; incident-response, consent, hospital-discharge / benefits apps | each app's demo test, on device, with no network |

## 6. Not in this layer

How an app looks, what it asks a person, how it stores its library: the
apps decide (`docs/10` §6). This layer only makes the engine and a local
judge available on each platform.
