# Pyramid solver reference fixtures

The two complete-deck fixtures in `tests/fixtures/pyramid-planning/upstream-15.json` and
`tests/fixtures/pyramid-planning/upstream-44.json` adapt the `solve-minimal` and
`solve-44-step-solution` tests from Mitchell Chung's **pyramid-solver** at revision
`c6dde73fd27ec817e2f1329f2a761b384cd8c663`:

- [Upstream repository](https://github.com/mchung94/pyramid-solver)
- [Pinned original tests](https://github.com/mchung94/pyramid-solver/blob/c6dde73fd27ec817e2f1329f2a761b384cd8c663/t/pyramid-solver-tests.lisp)
- [Retained MIT licence](pyramid-solver-LICENSE)

Copyright (c) 2017 Mitchell Chung. The complete MIT permission and warranty
notice is retained beside this file. The adaptation preserves all 52 identities
and their order, translates shorthand ranks/suits into xSAR schema-2 card records,
and labels the resulting documents as synthetic fixtures. They provide expected
optimal logical costs of 15 and 44 under the agreed stock-top/waste-top rules,
two recycles and one cost per removal, Draw or Recycle.

The Pyramid rules, complementary-rank lower bound and conservative complement
pruning in `src/games/pyramid/rules.rs` also retain their adaptation provenance
from this pinned upstream. They were moved from the accepted console implementation
without changing its rules profile. Generic search mechanics are separate from
this Pyramid adapter.

They are algorithm/regression inputs rather than captured deals or live win
evidence. Our independent replay validator simulates explicit stock/waste piles
and checks each returned action; it does not reuse search transition code.
