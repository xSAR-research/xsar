# xSAR 0.5.101 verification — host-verified M10 support

Updated: 19 October 2026. This record covers the approved reusable planning and
execution slice used by qmp-qemu-socket M10. **0.5.101 is host verified**: all
required host checks passed. The agreed minor increase and one build increment
are applied once. Application M10 is Awaiting verification because Beast live
behaviour remains unverified; M11 remains Planned. No installation or promotion
is claimed.

## Baseline and ownership

The inspected baseline was xSAR **0.4.100**, clean at
`00986b8d621803fc98da85616444fc76c14638a9`. The baseline index SHA-256 was
`8ae82c5fbcdc791ea66a47ee17924147ccf57ff8c08223d6a33b738126e7e73c`.
The private source/hash/index snapshot is
`/home/charlie/tmp/pyramid-m10-k4g2jqq2/baseline.json`.

This slice applies one agreed minor increase and one qualifying build increment
to 0.5.101. Build is never reset. qmp-qemu-socket independently earned 2.5.102;
the accepted basic-route-planning **0.8.4 is frozen byte-for-byte**, including its
already dirty accepted work. No console migration or application-to-console
dependency is included. A sibling path is not a revision pin.

`search=[]` owns generic bounded deterministic unit-cost A*/reference Dijkstra.
`execution=[]` owns generic finite observed-operation sequencing; both use only
`std`. `pyramid_rules=[pyramid,dep:thiserror]` owns checked planning types,
accepted rules and independent explicit-pile replay.
`pyramid_search=[pyramid_rules,search]` adapts those rules to the shared engine.
Defaults stay dependency-free. JSON, transport, images, native geometry and
input authority are absent from these pure features.

The app consumes this implementation directly for input-free route calculation
and guarded observed execution. It owns same-session restored-origin receipts,
exact export-byte binding, STOP/context, calibrated observations, transport
probes and every gesture. Generic acknowledgement never proves an effect.

## Retained focused results

The focused host run in
`/home/charlie/tmp/pyramid-m10-k4g2jqq2/xsar-focused.log` passed **37 tests,
zero failed, zero ignored**:

| Scope | Passed | Evidence covered |
| --- | --- | --- |
| `tests/search_generic.rs` | 14 | Non-card public consumer, more than 64 successors, deterministic ties, immutable predecessor reopening, actual generic state/action memory accounting, cancellation between successor steps and distinct interruption/proof outcomes. |
| `tests/execution_generic.rs` | 11 | Non-card finite operations, whole-action reservation, single-use/cross-run tickets, authority/time refusal, failed/uncertain delivery, STOP between pair gestures, phase/global budgets and no replay. |
| `tests/pyramid_planning.rs` | 12 | Licensed 15/44 minimum costs, manageable Dijkstra equivalence, separate accepted 48/49-action routes, independent replay, two-recycle turnover and illegal/partial/bounded-route refusals. |

The v7 oracle has 49 actions: 24 MATCH, 20 MOVE, 4 KING and 1 RECYCLE; independent
replay ends with tableau and both piles empty. V6's separate accepted route has
48 actions. Fixture provenance, copied original bytes and transcribed v7 trace
are recorded in [the fixture README](../tests/fixtures/pyramid-planning/README.md).
These are retained offline inputs, not a fresh scan or automated live acceptance.

The integrated application binary's all-feature check also passed; its current
focused controller/UI and native-observer results are recorded in the
[consumer verification record](../../qmp-qemu-socket/docs/Verification-2.5.102.md).
Final full consumer revalidation also passed, as recorded below.

The added `tests/search_execution.rs` is a combined public non-card consumer:
search finds SetSeven/Apply and the neutral sequencer verifies both logical
actions through Focus/WriteSeven/PressApply. It selects only `search` and
`execution`, without cards, JSON, images or transport. It is additional coverage
passed in both the full suite and independent `search,execution` configuration.
It is not part of the earlier focused 37 above.

## Completed full host verification

The final record is `/home/charlie/tmp/pyramid-m10-k4g2jqq2/verification.json`;
`commands.json` retains every exact argv, working directory, result and log.
`verify.py` ran all required commands; `finalise.py` completed the preservation
comparison after the telemetry-only helper exception described below.

Actual toolchain: `rustc 1.101.0-nightly (8006091eb 2026-10-17)`, full compiler
revision `8006091eb71ae7d9e9bccb66c6122c9d68af4466`, LLVM 23.1.4;
`cargo 1.101.0-nightly (3960fa69b 2026-10-12)`. Host/target:
`x86_64-unknown-linux-gnu`. Edition 2024 and declared MSRV 1.101.0 are unchanged;
these checks establish the selected nightly, not additional compiler support.

All Cargo checks used `cargo +nightly`, `--locked --offline` and explicit
`--target x86_64-unknown-linux-gnu`, except dependency-tree inspection, which
used the same locked/offline selection without a target option.

| Required verification | Actual result and retained log |
| --- | --- |
| New-module scoped formatter | Passed `rustfmt +nightly --edition 2024` with the owning formatter settings and `skip_children=true`; `scoped-format.log`. |
| Locked `check --all-targets --all-features` | Passed; `xsar-check.log`. |
| `test --all-features -- --include-ignored` | **256 passed, 0 failed, 0 ignored**, including doctests; `xsar-tests.log`. |
| Strict private `doc --all-features --no-deps --document-private-items` | Passed with `RUSTDOCFLAGS='-D warnings -D missing_docs'`; `xsar-rustdoc.log`. |
| `clippy --all-targets --all-features -- -D warnings` | Passed; `xsar-clippy.log`. |
| New production module safety lints | Passed with `--no-default-features --features search,execution,pyramid_search --lib -- -D warnings`; module-level denies cover indexing, unwrap, expect and panic in new production code; `xsar-safety.log`. |
| `build --all-features --release` | Passed; `xsar-release.log`. |
| Real application-consumer required checks | Passed check, **907 tests, 0 failed/ignored**, strict private docs, Clippy, native release and executable `--help`; [consumer record](../../qmp-qemu-socket/docs/Verification-2.5.102.md). |
| Compiler inputs/index/console/evidence preservation | Passed final hash comparison; all three Git indices and every frozen console file unchanged. See exceptions below. |
| Installed/running executable and live guest acceptance | Not established by host checks. No guest trial or installation performed; M11 Planned. |

The independent feature matrix used `test --no-default-features`, selecting only
the named feature(s), and `-- --include-ignored`:

| Features | Passed | Failed/ignored | Log |
| --- | --- | --- | --- |
| Default (`[]`) | 14 | 0/0 | `xsar-feature-default.log` |
| `search` | 28 | 0/0 | `xsar-feature-search.log` |
| `execution` | 25 | 0/0 | `xsar-feature-execution.log` |
| `search,execution` | 40 | 0/0 | `xsar-feature-search-execution.log` |
| `pyramid_rules` | 57 | 0/0 | `xsar-feature-pyramid_rules.log` |
| `pyramid_search` | 71 | 0/0 | `xsar-feature-pyramid_search.log` |

These configurations overlap; their counts are not additional unique tests to
sum with the 256-test full suite. The Pyramid-only configurations pass without
JSON. `cargo tree` for default, search, execution and search+execution each
contained only xSAR, confirming no external dependencies for the neutral modules.

## First failures, formatting and preservation

The first complete attempt stopped at strict rustdoc: 23 pre-existing missing
variant/field contracts in `src/lib.rs`. Documentation was added without changing
telemetry behaviour, then the required check passed. A subsequent optional
crate-wide safety probe flagged four existing geometry/complete-deal invariant
accesses. Those legacy accesses were retained; this record claims mechanically
denied indexing/unwrap/expect/panic for the new production modules, rather than
claiming that the entire legacy crate passes those additional lints. The
application's new style/field documentation errors were corrected before final
full checks. First failures remain in `first-full-failure/`,
`second-full-failure/` and `targeted-clippy-first-failure.txt`.

New modules passed their scoped formatter. `format-parent-check.json` reports
only two inherited first-`if` blank-line differences in the application's
`ensure_strategy_input_authorised` and `run_preparation_capture`; HEAD/current
spacing matches and is preserved as required by the standing instructions.
Package-wide baseline formatting differences remain; no blanket or sibling
reformat is claimed.

All required commands passed before `verify.py`'s final preservation helper
asserted on automatic documentation-read telemetry. `finalise.py` rechecked the
exact verified compiler inputs, all three Git indices, all frozen console files
and unrelated source/evidence. The only allowed operational differences were
app `.trigger-tree/history.jsonl` and
`.trigger-tree/sessions/b1065bcab784f174a496f2aa5538322e.json`; their before/after
hashes and classification are retained in `verification.json`. They were left
intact. No source/evidence drift or changed compiler input followed the checks;
the helper correction required no test rerun.

## Explicit M10 review paths

The baseline xSAR checkout was clean. Review these paths explicitly; do not use
this list as staging, commit, push or registry-publication authority:

```text
Cargo.toml
Cargo.lock
README.md
CHANGELOG.md
docs/Verification-0.5.101.md
src/lib.rs
src/search.rs
src/execution.rs
src/games/pyramid.rs
src/games/pyramid/planning.rs
src/games/pyramid/rules.rs
src/games/pyramid/replay.rs
src/games/pyramid/search.rs
tests/search_generic.rs
tests/execution_generic.rs
tests/search_execution.rs
tests/pyramid_planning.rs
tests/fixtures/pyramid-planning/README.md
tests/fixtures/pyramid-planning/upstream-15.json
tests/fixtures/pyramid-planning/upstream-44.json
tests/fixtures/pyramid-planning/pyramid-recon-20261018-v6-01.json
tests/fixtures/pyramid-planning/pyramid-recon-20261018-v6-01-solver.csv
tests/fixtures/pyramid-planning/pyramid-recon-20261019-v7-01.json
tests/fixtures/pyramid-planning/pyramid-recon-20261019-v7-01-solver.csv
third_party/README.md
third_party/pyramid-solver-LICENSE
```

The [third-party record](../third_party/README.md) retains the full upstream MIT
notice and pinned revision `c6dde73fd27ec817e2f1329f2a761b384cd8c663`.
Promotion, if separately authorised, must establish the actual crate revision
first, agree the consumer dependency/pin and revalidate that consumer.

## Proposed promotion description

Proposed Conventional Commit title, for review only:

```text
feat(planning): add bounded search and observed execution
```

Proposed body:

```text
Add optional std-only generic search and finite observed-operation sequencing,
plus pure Pyramid planning/rules/independent replay and licensed regressions.
Integrate the real qmp-qemu-socket consumer directly while freezing console 0.8.4.

Apply the agreed xSAR minor milestone and one verified build increment: 0.5.101.
Validation: 256 all-feature tests, feature matrix, strict private docs, Clippy,
new-module safety lints, scoped formatting, release and full app consumer checks.
Live guest acceptance and registry publication remain separate decisions.
```

No staging, commit, push or publication was performed.

## Code Analysis

Search accepts only copied/hashable states, copied actions and finite synchronous
successors. Fallible growing collections and checked counters bound engine work;
estimated storage is not an RSS guarantee, and caller callbacks/iterators own
their own memory and termination obligations. A stopped search proves neither
unsolvability nor optimality. Weighted costs are unsupported.

Pyramid types preserve fixed 28-slot tableau/24-slot stock identities, checked
one-based public mappings and independent replay with a 4,096-action ceiling.
Pair exposure is resolved before either removal. Unknown recognition remains a
consumer refusal; it is never filled by elimination.

The generic sequencer cannot grant input authority. Private single-use tickets,
monotonic authority/time checks, whole-action gesture reservations and terminal
failure prevent automatic input replay. Cooperative checks cannot cancel an I/O
operation or callback already in progress. Only the application may establish
fresh supported evidence and perform guarded operations. No live acceptance or
signature weakening follows from synthetic or retained-fixture success.
