# Hierarchical walk against flat discover (pre-registered, 2026-10-31)

Written before any walk run. The walk is onto's inference engine (frames,
the walk's own gate, escalation, frame records); discover is one flat
Choice per package.

- **Same input**: the package descriptions, dependencies and module
  blueprints of the flat runs (`../sysmdl`, `teammates.doc.json`,
  `argouml.cookbook.json`); groups from `hierarchy.json`, described only
  by their document's name and their members' descriptions.
- **Judge**: Jev. Systems: TEAMMATES and ArgoUML; names visible and
  hidden. Proposer off (mock, closed world): no structure is learned.
- **Walk**: CodeUnit → group (Choice) → module (Choice); a group of one
  module is a direct arrow; modules the hierarchy leaves at the root are
  direct arrows too. Run at threshold 0 so every walk ends at a module
  unless the judge answers none; the gate is applied afterwards from the
  frame records.
- **Primary metric**: class-level accuracy (a class is right when its
  package's walk ends at its expert module), as for the flat runs.
- **Rule**: the hierarchy **helps** if hidden accuracy is at least 0.05
  above flat discover's on both systems; **hurts** if 0.05 below on
  either; otherwise no difference. Visible is reported, not ruled on.
- **Also reported**: where wrong walks went wrong (group or module step),
  per-frame calibration (ECE), and the gate at path p ≥ 0.7 (path p =
  product of the step p's): coverage and accuracy of adopted packages,
  against flat discover's gate at p ≥ 0.7 from its records.
