# onto — Join Semantics

Status: **design, not built — 2026-09-23.** How parallel branches (from a
noul fork) recombine. Written before implementation, against `60c0008`.

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

Provenance: the join produces one frame record whose causal links are
**all** arriving branches' last records (`after` becomes a list), so the
disposition graph stays a DAG with the join as a merge node.

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

## 6. Build order

1. `join` declarations parsed and validated (join object reachable from a
   noul fork; `gate authority` names a fork arrow; `export` tokens declared).
2. Runtime: join table keyed by (fork record, join object); All, Gate, Race;
   `incomplete_join` and `blocked_by_gate`; multi-parent frame records.
3. Beneficiary view: "the parts of your case came back together at …".
4. Later: KOfN, Accumulate (needs dependence from the provenance DAG:
   branches sharing ancestry must not double-count), coincidence windows,
   capability modes (persistent, linear, affine, mergeable).
