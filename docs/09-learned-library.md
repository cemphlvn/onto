# onto — The Learned-Structure Library

Status: **built and measured live — 2026-09-24** (plan step B). Follows
`docs/07-raster-findings.md` §1, which found learned arrows that were
never taken afterwards, and the question of what to do with them.

Learned structure is **never deleted**. It is kept in a library, and each
arrow has a place in it:

| state | in the graph | recalled at a gap | who puts it there |
|---|---|---|---|
| **active** | yes: a candidate at its frame, part of every judgment there | — | the open world (admitted), a recall, a person (`--activate`) |
| **dormant** | no: no presence in any judgment | yes, before a catalogue or a model | a person (`onto learned --dormant`), after the measurements below |
| **retired** | no | no | a person (`--retire`); kept as history |

The state lives in the learned layer itself (`<stem>.learned.jsonl`,
`state` and `note` per entry): one source of truth. Every load still
replays active arrows through the structural proofs (D38); an active
arrow whose source exists only through a dormant one waits in the
library too and comes back with it.

## 1. Presence: why "never taken" is not "no effect"

An arrow at a frame is a candidate in every judgment there, taken or not.
Its presence can shift the distribution over the others, absorb a case,
or keep a case from escalating. Two measurements, both advisory:

- `onto learned FILE --usage d1.jsonl,d2.jsonl`: per learned arrow, how
  often it was judged, how often taken, its mean judgment; an active arrow
  judged at least `--min-judged` times and never taken gets a
  **recommendation** (nothing changes automatically).
- `onto replay FILE d.jsonl --without-arrow A`: the same recorded question
  without A (every judged record where A was a candidate), next to a
  baseline: which decisions change. `--with-arrow A` asks with a dormant
  or absent arrow added: would it have been taken?

## 2. Recall: library → catalogue → model

At a **structure gap** on an `open_world` frame (only there: elsewhere a
person decides), the order is:

1. **library**: dormant arrows from this frame are admitted again (the
   structural proofs run against the current graph); the frame is judged
   again with them present. The layer marks them active: a recall is
   evidence the structure is needed. Active structure that hung from them
   is restored with them.
2. **catalogue**: transport from a functor's empty fibers (`docs/05` §4).
3. **model**: the proposer, one call per gap (single-flight).

**Stale judgments.** A walk judged before another walk extended its frame
(by a recall, a transport, or an admitted proposal) meets its gap on a
graph that has grown. It judges again on the current graph before asking
anyone. The check runs after its own recall attempt, so a concurrent
recall has already written the graph.

## 3. Tests (live, Jev; OpenRouter for proposals)

### Presence effect — benefits-assembly + catalogue

*A person applying across programmes.* Two runs of the application
stream learned `maternity` and `bereavement` (transported from the
catalogue) and `review_contract_nonrenewal` (proposer).

| arrow | judged · taken · mean | `--without-arrow` (8 records) |
|---|---|---|
| `bereavement` | 9 · **0** · 0.00 | **0 decisions change**; every judgment identical → recommended for dormancy |
| `maternity` | 9 · 2 · 0.22 | the pregnant case **escalates** (none of these 0.73, confidence 1.00 → 0.66); no other case moves |

Then `bereavement` was made dormant and two bereaved applicants arrived
(a widow raising two children; a 19-year-old whose mother paid the rent).
**Nothing was recalled**: Circumstances did not escalate. The widow went
to `none_apply` (0.88), the student to `is_carer` (0.77). `replay
--with-arrow bereavement` on those records: **both take it at 1.00**.

The lesson, and why dormancy is a person's act:

- "Never taken" measured the case mix of the runs, not the need: no one
  bereaved had applied.
- A dormant arrow is recalled only at a gap, and **its absence does not
  always make one**: a catch-all option (`none_apply`) or a near miss
  (`is_carer`) absorbs the case, confidently.
- So the recommendation now says so, and a dormant arrow's listing asks
  for an audit: `replay --with-arrow` on later records at its frame. Here
  the audit sent it back (`--activate bereavement`).

### Recall — support-commons, the parcel stream

*Shoppers with parcel problems.* Eight tickets (five about orders), the
open world, three times on one layer:

| run | library | proposer calls at Ticket | tickets stopped at Ticket |
|---|---|---|---|
| 1 | empty: the proposer learns `delivery`, `report_damage`, `handle_return` (and 5 more below them) | 3 (12.7 s) | 0 |
| 2 | the three Ticket arrows dormant; recall, no stale check yet | 1 (4.2 s) | **4** |
| 3 | the same, with stale judgments judged again | **0** | **0** |

In run 2 the library answered the first ticket to reach the gap (w6),
but four tickets judged at the same moment reached their gaps 10 ms
later, on the graph they were judged on and with the library already
empty. One asked the proposer (4.2 s, nothing admitted: its answer
existed), and the others waited on that call. Run 3: one recall (w3.1),
four walks judged the grown frame again, four dependent arrows
(`change_delivery_address`, …) restored with their parents, and every
parcel ticket reached its team without a model call at Ticket. (Proposer
calls remain deeper, at open leaf frames the open world keeps
extending.)

## 4. What is automatic and what is not

- Automatic: recall at an open-world structure gap (it admits nothing new
  that the open world could not already learn, and passes the same
  proofs); reactivation of what was recalled; restoring its dependents.
- Never automatic: dormancy and retirement (a person, `onto learned`),
  anything at `assured` or `sealed` frames. The measurements recommend.

Tests: `crates/onto-runtime/tests/library.rs` (recall before the proposer;
no presence while dormant; retired and empty libraries; walks judged
before a recall judge again; dependents restored).
