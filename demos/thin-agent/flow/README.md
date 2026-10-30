# onto's architecture pipeline as a flow, checked by onto

`thinagent-ci.flow` is the OSIL drawing of the pipeline (DeriveDeps →
Discover → Calibrate → Gate → AskHuman → Adopt → FunctorCheck → Record /
CIGate / RenderView), copied from the OSIL draft. `flow2onto.py` turns it
into an onto category (`flow.onto`, generated): data values are objects,
each operator is an arrow from every input to every output, so a path is
a route data can take.

```
python3 flow2onto.py thinagent-ci.flow rules.onto-part roles.json > flow.onto
onto check flow.onto                 # the rules, proved as invariants
onto functor flow.onto FlowRoles     # the flow mapped into ThinAgentArchitecture
python3 counterexamples.py           # each rule must bite
```

| rule (`rules.onto-part`) | invariant | where it was found (`../EVALS.md`) |
|---|---|---|
| NeverGuess | `via: deferred -> roles through answers` | a failed or deferred judgment is never guessed at (commit 8680a74) |
| MapBeforeCheck | `never: standard -> proposal`; `via: proposal -> verdict through roles` | mapping under the path constraint lost recall (Result 3) |
| GateInSubstrate | `via: proposal -> roles through scored`; `via: proposal -> roles through accepted \| deferred` | errors cluster at low p (Results 2, 5) |
| BlueprintFrozenBeforeTest | `never: verdict \| decision \| record -> blueprint` | the clean ArgoUML protocol (Result 5); freezing in time is a commit's job, this forbids the data route |
| CI | `via: proposal -> decision through verdict` | no merge decision without the functor check |

`counterexamples.py` breaks each rule once; onto refuses every mutant
with the broken invariant's name. Feeding the verdict back into the
blueprint breaks two rules: the standard then reaches the proposal
(standard → verdict → blueprint → proposal), and onto reports that one
first, so `never: verdict -> blueprint` is in practice implied by
MapBeforeCheck.

The functor into `ThinAgentArchitecture` holds its invariants. Authority
and contracts are reported, not required: the standard's
`entry Substrate: needs Contracted` reads "an agent's answer enters the
substrate only through the contract", but in the flow the substrate also
takes inputs no agent produced (sources, the standard, the cost, a
person's answers). Mapping the flow showed the standard was missing the
request direction; `request: Substrate -> Agent` was added for it.

Built operators: DeriveDeps (`../derive.py`), Discover, FunctorCheck,
Adopt (`onto promote`). In walks only, not yet in discover: Gate,
AskHuman, Record. Only in the eval scripts: Calibrate. Not built: CIGate.
