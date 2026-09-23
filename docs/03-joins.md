# onto — Join Semantics

Status: **built — 2026-09-23** (designed first against `60c0008`). How
parallel branches from a noul fork recombine. Implementation:
`crates/onto-runtime/src/joins.rs`; syntax and validation in core.

**As built:**
- `join X: all | race | gate authority A export T, …`; the gate's
  authority must be an arrow leaving a noul frame, and exported tokens
  must be declared capabilities (the policy validator of
  `docs/04-trust-model.md`).
- Walks carry a stack of forks (innermost last); a walk arriving at a
  join object joins its innermost fork's siblings; walks that never
  forked pass through.
- **Deadlock freedom:** a walk waits only on siblings that are still
  running; a sibling that ended, or waits at another join, resolves the
  join as incomplete or blocked. Tested under timeouts; stable over
  repeated runs.
- Race losers are not interrupted; they end when they arrive.
- Every join writes a frame record whose `merged_from` lists the other
  branches' last records: the merge nodes of the disposition graph.
- Live (incident-graph, `join Mitigated: all`): a deploy branch reached
  Mitigated and waited 5.3 s; its security sibling stopped at Rotate; the
  join escalated `incomplete_join`. An incident with an unfinished aspect
  is not reported mitigated. A successful all-join is covered by tests,
  not yet shown live: when a case describes actions already taken, the
  judge hesitates at the arrows for those actions (the graph models what
  to do, not what was done).

## 1. Principle

Joining is not collapsing branches. It is a **typed integration
operator** chosen by the relationship between the branches:

| relationship | question the join answers | policy |
|---|---|---|
| complementary work that must all finish | have all necessary processes completed? | **All** |
| content that needs authorization | is this process authorized to manifest? | **Gate** |
| incompatible alternatives | which process manifests? | **Race** |
| corroborating evidence | is there enough support? | later: KOfN, Accumulate |
| contradictory | — | escalate, never merge |

Inspiration, not a claim about brains: dendritic coincidence detection
(All), apical–basal gating in layer-5 pyramidal cells, where a burst
needs feedforward input and feedback context together (Larkum 1999, 2013:
Gate), evidence accumulation to a bound where the first accumulator to
threshold wins (Gold & Shadlen: Race), and reliability-weighted cue
integration, which assumes independent cues (Ernst & Banks 2002: why
Accumulate needs dependence information). The semantics come from
workflow control-flow patterns (van der Aalst et al.): AND-join,
deferred choice / discriminator, N-out-of-M.

## 2. Scope: structured joins only

A join combines **siblings of one fork**: the walk that forked and the
branches it spawned (the fork's frame record lists them). The set of
walks that may still arrive is therefore always known. This avoids the
general OR-join, whose semantics are notoriously hard because it must
know whether more branches can still arrive. Nested forks join inner
siblings first.

## 3. Syntax

```
join Mitigated: all;                                   # AND-join of the fork's branches
join Marketing: gate authority check_consent export ConsentGrant;
join Response:  race;
```

`gate authority A` names the fork arrow that spawned the authority
branch; the other sibling(s) carry content. `export` is an explicit
allowlist of tokens the authority branch hands over.

## 4. Runtime semantics

A walk arriving at a join object waits there (holding no frame claim)
until the policy resolves for its fork's siblings.

| policy | resolves when | continuation | tokens out | others |
|---|---|---|---|---|
| **All** | every sibling has arrived | one walk (lowest id) continues | $\bigcap_i T_i$ | end as `joined into w…` |
| **Gate** | the authority branch has arrived | the content walk continues | $(T_c \cap T_a) \cup (T_a \cap \text{Export})$ | authority ends as `authorized w…` |
| **Race** | the first sibling arrives | the first arrival continues with its own tokens | $T_{\text{first}}$ | later arrivals end as `lost race to w…` (potentialities) |

Failure: under **All**, a sibling that ends elsewhere (terminal,
escalation, failure) makes the join incomplete: the waiting walks
escalate at the join object with reason `incomplete_join`, listing the
missing branch. Under **Gate**, an authority branch that ends elsewhere
leaves the content walk `blocked_by_gate`. Intersection is the default
because it is least privilege: a joined walk holds only what every
branch holds. Gate is the one asymmetric case, where authority is
transferred, and only through the allowlist.

Provenance: the join produces one frame record whose `after` is the
continuing walk's previous record and whose `merged_from` lists every
other arriving branch's last record, so the disposition graph stays a
DAG with the join as a merge node.

## 5. Laws stay sound

The arrow effect $\text{eff}(S) = (S \setminus R) \cup E$ distributes over
intersection: $\text{eff}(A \cap B) = \text{eff}(A) \cap \text{eff}(B)$, and
an entry gate ($\text{needs} \subseteq \text{eff}(S)$) passes for $A \cap B$
exactly when it passes for both. So a walk continuing with $A \cap B$
arrives downstream with at most what both branch walks would have held.
Therefore the existing analysis, which treats a join object as an
ordinary arrival of each branch, stays **sound for MUST laws** under All
(joined tokens are the intersection), Gate (joined tokens are a subset of
the authority branch's, and contain $T_c \cap T_a$), and Race (the
continuation is one branch's own state). MAY sets over-approximate. No
existing law is weakened by adding joins.

## 6. Build order (1–3 done)

1. `join` declarations parsed and validated (`gate authority` names an
   arrow leaving a noul frame; `export` tokens declared). Not validated:
   that a join object is reachable from a noul fork (walks that never
   forked simply pass through).
2. Runtime: join table keyed by (fork record, join object); All, Gate, Race;
   `incomplete_join` and `blocked_by_gate`; multi-parent frame records.
3. Beneficiary view: "the parts of your case came back together at …".
4. Later: KOfN, Accumulate (needs dependence from the provenance DAG:
   branches sharing ancestry must not double-count), coincidence windows,
   capability modes (persistent, linear, affine, mergeable).
