# JabRef 3.7 (SAEroCon): the thin-agent evals

Reproduces the JabRef results in `../../EVALS.md`.

```
# the data (not committed): sources, intended architecture, violations
git clone --depth 1 --filter=blob:none --sparse \
    https://github.com/sebastianherold/SAEroConRepo.git data/repo
git -C data/repo sparse-checkout set systems/JabRef/architectures \
    systems/JabRef/sources/3.7/src/main/java

cargo build -p onto-cli --release          # from the repository root

# layer 1: reader + expert mapping, graded against HUSACCT's violations
uv run --with xlrd python jabref_eval.py

# layer 2: discover proposes the mapping (needs TYPESAFE_API_KEY)
uv run --with xlrd python layer2.py structure --no-roles --framing=arch
uv run --with xlrd python layer2.py meaning --no-roles --framing=arch
# ablations: --target=names, --source=package, drop --no-roles or
# --framing=arch; --rep=N repeats; --rescore re-grades a saved run
python ablation.py
```

| file | what |
|---|---|
| `jabref_eval.py` | regex Java reader; expert mapping and rules from the HUSACCT spreadsheets; P/R against labelled violations |
| `layer2.py` | packages + dependencies → a category; modules described by JabRef's own documentation → a category; `onto discover`; graded at class level (P over mapped, R over all), with documentation-rule variants and calibration (Brier, ECE, Platt leave-one-out) |
| `ablation.py` | each part removed from the full configuration, deltas |
