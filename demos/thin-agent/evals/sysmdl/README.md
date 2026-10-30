# The Olsson et al. systems: onto discover against s4rdm3x's expert mappings

`sysmdl_eval.py` reads a `.sysmdl` (modules, a regex per module, allowed
relations) from [s4rdm3x](https://github.com/tobias-dv-lnu/s4rdm3x)
(`data/systems/*/`), the mappings Olsson et al. (JSS, arXiv 2109.09525)
evaluated, and the system's Java sources.

```
mkdir -p data && for f in teammates/teammates.sysmdl; do
  gh api repos/tobias-dv-lnu/s4rdm3x/contents/data/systems/$f --jq .content | base64 -d > data/$(basename $f)
done
# TEAMMATES 5.110 sources: SAEroConRepo systems/TEAMMATES/sources/5.110/src/{main,client,test}/java
python3 sysmdl_eval.py data/teammates.sysmdl SRC/main/java SRC/client/java SRC/test/java --names=visible
python3 sysmdl_eval.py data/teammates.sysmdl SRC/main/java SRC/client/java SRC/test/java --names=hidden
```

`--names=hidden` replaces package paths with shuffled ids: in 7 of the 8
systems the expert mapping is a package-prefix rule, so with paths
visible the answer is in the input. Hidden, only class names and
dependencies remain.
