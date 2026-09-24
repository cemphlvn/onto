# onto template optimizer

An early prototype for optimizing reusable text templates with explicit policy and measured evidence. Inspired by the parent [onto](https://github.com/cemphlvn/onto) runtime: declared constraints remain authoritative; candidate variants are hypotheses; rejected candidates remain visible in the decision record.

**Status:** standalone prototype. It does not call onto yet. onto's shared application API and Python bindings are listed as future work in its architecture document. Do not describe this package as an onto runtime integration.

## Install and try

```bash
cd frameworks/template-optimizer
python -m pip install -e .
python examples/basic.py
python -m unittest discover -s tests -v
```

Define `Case` records, a baseline `Candidate`, candidate rewrites, and `Policy`. Supply `run(rendered_text, case)` to execute a template and `score(output, case)` to return a larger-is-better number. `Optimizer.optimize` evaluates baseline and candidate on the **same cases**, checks declared constraints before running the candidate, and admits it if the lower end of an approximate paired 95% interval exceeds `minimum_gain`. Use `log_path` to append decisions as JSONL.

The metric and cases must come from outside the candidate generator. Reserve an untouched test set for final validation; repeatedly using these optimization cases will overfit them. A confidence bound over a tiny or nonrepresentative set does not prove generalization. Calls to `run` may have side effects: supply a sandboxed runner.

The current benchmark loop is deliberately small: it sequentially evaluates candidates against the current incumbent. Its approximate interval is a screening rule, not a statistical guarantee under adaptive search. Add a separate final evaluation before deploying a winning template.

## Onto integration plan

| This prototype | onto concept | Next integration |
| --- | --- | --- |
| `Policy` | declared category and admission policy | load rules through onto app API |
| `Candidate` | proposed structure, with provenance | map variant operation to typed arrow |
| `Policy.check` | hard checks before admission | delegate structural proofs to onto |
| `Decision` | review and disposition record | persist frame record and evidence |
| `run` and `score` | external observations | attach attested evaluation results |

Template quality is application-defined; onto should govern what can change, while an evaluator measures whether the change helped. Template text itself is data, not a capability issuer. An LLM proposer, cross-validation, multiple-comparison control, and genuine onto bindings are future work.

## License

MIT. See `LICENSE`.
