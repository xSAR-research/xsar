# Changelog

## 0.8.104 — publication summary, 23 October 2026

- Deliver reusable, optional search and execution foundations added since
  0.4.100: deterministic A*/Dijkstra, weighted search options, synchronous progress,
  immutable reconstruction and observed-action sequencing without transport or
  consumer input authority.
- Add checked Pyramid rules, planning, independent replay and scored best-effort
  search. Prefer a full goal; otherwise maximise tableau removals and minimise
  equal-score route cost, with exhaustive and bounded proofs kept distinct.
- Support explicit action costs through the default-one generic contract and
  Pyramid cost selection. CardMatches charges MATCH/KING one and DRAW/RECYCLE
  zero; existing unit-cost APIs and dependency-free default features remain.
- Preserve checked allocation/arithmetic, cancellation, aggregate fallback budgets
  and caller-owned replay. The application's operator-controlled limits and
  challenge policy do not change the crate's bounded defaults or scoring.
- Retain qualification for 289 shared tests, six feature configurations
  (14/55/25/67/57/99) and the 80-test private console consumer. The current
  qmp-qemu-socket 2.9.133 consumer passed 1,359 native tests; unchanged shared
  checks are retained by exact input/log hashes rather than rerun for this
  documentation update. Host checks do not establish guest acceptance.
- Include the upstream MIT notice and attribution. This documentation update
  earns no version increment and does not publish the crate to crates.io.
- Publish the library before its paired consumers; retain the established relative
  sibling dependencies without introducing temporary source overrides.

<!-- qualification104:start -->
## 0.8.104 — explicit checked objective costs, host verified

- Add default-one SearchProblem::action_cost; preserve existing callers.
- Keep checked cumulative cost separate from immutable route depth, lexicographic
  state dominance, independent reconstruction and aggregate fallback budgets.
- Add explicit PyramidCost and solve_best_effort_with_cost. Existing Pyramid
  APIs remain unit logical-action cost; CardMatches assigns MATCH/KING one and
  MOVE/RECYCLE zero. Tableau-removal score excludes stock-only removals.
- Cover cheaper longer routes, zero-cost cycles/reopening, stale histories,
  reconstruction, proof limits, overflow and callback cancellation.

Shared 289, matrix 14/55/25/67/57/99, real consumer app 1260 and private console
80 passed with zero failed/ignored/filtered; strict required checks/release,
scoped formatting and neutral dependency trees passed. Console originals remain
frozen. No crate publication or Git promotion occurred. Fresh guest acceptance
belongs to the consumer. See the [paired record](../qmp-qemu-socket/docs/Verification-2.8.119.md).
<!-- qualification104:end -->

## Unreleased

<!-- qualification103:start -->
## 0.7.103 — scored fallback search, host verified

- Add generic `BestEffortProblem`, `BestEffortOutcome`, `PartialProof` and
  `solve_best_effort`, plus a pure Pyramid `solve_best_effort_with_options` adapter.
  Preserve every existing complete-route API and console contract.
- Prefer full goals, then maximise score and minimise equal-score action cost.
  Retain useful legal prefixes before a resource cutoff; use an unpruned fallback
  with remaining aggregate state/expansion/time budgets. Distinguish exhaustive
  proof from bounded best found and withhold actions on cancellation/fatal failures.
- Preserve dependency-free defaults, optional features, checked allocation,
  cumulative progress and consumer-owned replay/input authority.

Full shared checks passed 278/0/0; six feature configurations passed 14/46/25/58/57/89
(overlapping); real app 1,218/0/0 and private console 80/0/0, all zero filtered.
Strict private docs/Clippy/release and neutral dependency checks passed. The original
app 1,215/3/0 failures are retained; only outdated app test expectations changed and
the sealed shared successes were reused. Version 0.7.103 is qualified. Final paired
documentation/preservation audit passed: 12 documents, 309 local links and 40 review
paths, with exercised inputs preserved. Live gameplay acceptance is separate.
<!-- qualification103:end -->

## 0.6.102 — additive search options and progress, host verified

- Add `SearchOptions` with positive heuristic weight and `solve_with_options`
  to generic search and the Pyramid adapter. Default/weight-one A* and existing
  `solve` signatures remain exact; Dijkstra ignores weight and guidance.
  Larger weights may return longer routes and do not guarantee faster search.
- Add borrowed synchronous `SearchProgress` callbacks: initial, at most once
  per 200 ms periodically, and one final snapshot on normal return. Keep actual
  work/time statistics, cancellation around callbacks, checked ordering/growth
  and all resource-stop/proof boundaries. No thread, queue or I/O is introduced.
- Preserve unit action costs, independent replay responsibility, old API callers,
  default 512 MiB storage policy, every feature/dependency contract, MSRV/edition
  and dependency-free default/search/execution configurations. App 2.6.108 owns
  its separate no-byte-cutoff policy and native session/input authority.

Complete host verification passed **266 tests, zero failed/ignored**, required
locked check, strict private docs, warnings-denied Clippy, scoped formatting and
all-feature release. Matrix counts default 14/search 37/execution 25/combined 49/
pyramid_rules 57/pyramid_search 80 overlap the full suite; neutral dependency
trees remain std-only. Real app 2.6.108 consumer passed 1,043 tests and required
checks. Private frozen-console-copy smoke passed 80 tests; original 0.8.4 console
was neither built nor changed. One agreed minor/build is earned. The
[paired verification record](../qmp-qemu-socket/docs/Verification-2.6.108.md)
retains exact commands, previous failures, the intentional interrupted first
attempt and complete passing attempt 0002. Native Continuous remains unavailable
in the consumer without positive progress/target evidence; host proof establishes
no guest acceptance or completed native repetition. No installation, promotion
or registry publication was performed; final documentation/preservation audit passed after handover (11 documents, 162 local links, 44 review paths).

## 0.5.101 — reusable planning and observed execution, host verified

- Add independent opt-in `search` and `execution` modules. Search is generic,
  deterministic, bounded unit-cost A*/reference Dijkstra with typed outcomes,
  cooperative cancellation and fallible collection growth. Execution consumes
  typed finite operations with single-use request tickets, authority epochs,
  separate delivery/effect evidence and terminal refusal without retry/resume.
- Add `pyramid_rules` and `pyramid_search` features. Move the accepted pure
  Pyramid planning types, checked mappings, rules, cached pruning and independent
  explicit-pile replay into the game adapter. Keep card identities in
  `CompleteDeal`; JSON, capture, calibrated coordinates and QMP authority stay
  outside these pure features.
- Retain licensed 15/44 fixtures and separate accepted v6 48-action/v7 49-action
  replay/optimal-cost regressions, including the complete upstream MIT notice.
  Generic search/execution tests use non-card consumers as well. Add a combined
  public consumer in `tests/search_execution.rs`: search finds a settings route
  and the neutral sequencer verifies its two actions/three gestures, with no
  cards, JSON, images or transport. It passed full and independent feature checks.
- Integrate the application directly with xSAR; preserve the accepted console
  0.8.4 byte-for-byte without an application dependency on that console.
- Preserve dependency-free defaults, existing feature contracts, edition, MSRV,
  JSON schemas, transport protocols and all calibration signatures.

Complete host verification passed **256 tests, zero failed/ignored**, including
ignored tests, plus the independent default/search/execution/combined/Pyramid
feature matrix. Locked check, strict private rustdoc, warnings-denied Clippy,
new-module production safety lints, scoped formatting and release passed. The
real qmp-qemu-socket consumer passed 907 tests and its required host checks.
[Verification-0.5.101.md](docs/Verification-0.5.101.md) retains commands, first
failures, formatting exceptions and preservation evidence. Version 0.5.101
applies the agreed minor increase and exactly one earned build increment.
No live guest acceptance, installation, commit, push or publication is claimed.

## 0.4.100 — numeric migration checkpoint (verification pending)

- Replace the candidate package suffix with Charlie's numeric
  `major.minor.build` policy, retaining the existing 0.4 line. Build 100 is a
  migration checkpoint, not an inferred count of historical tested revisions.
- Keep the crate's build counter independent of qmp-qemu-socket's counter.
  Advance it once for each changed revision that passes the required tests;
  major/minor transitions require Charlie's direction or approval.
- Synchronise package metadata, the crate's own lockfile entry and the current
  README label. No source behaviour, public API, features, dependencies or
  capture protocol changes are included. Required Rust verification is pending.

## 0.4.2-candidate.2 — development

- Add candidate 0.4.2-candidate.2: checked version-3 full-screen BGR8 headers,
  opaque RGBA reconstruction, explicit legacy/one-frame/persistent modes,
  correlated response metadata and independent monotonic timing boundaries.
- Bound connect and transaction stages; support cooperative cancellation during
  socket I/O and discard failed sessions without retry. Numeric endpoints avoid
  unbounded DNS; an active connect may still take up to five seconds.
- Preserve dependency-free defaults and the standalone tcp_capture feature;
  no region, compression, QMP input or image-recognition authority is added.

- Share TCP capture protocol version 2 with the Windows server: a 14-byte
  little-endian header with checked `u16` stride and one frame per connection.
- Bound packed BGRA rows to 65532 bytes / 16383 pixels, reject invalid signed
  values before narrowing, and validate frame geometry before reading pixels.
- Add boundary, malformed-header, full-HD color conversion and reconnect tests.
  Both endpoints must be rebuilt for the changed wire format.

## 0.4.2-candidate.1

- Add `image_matching::find_black_run_bottom_up` over a validated borrowed frame
  and rectangular search area. Visit rows bottom-up and pixels left-to-right;
  return the first qualifying run's leftmost full-frame coordinate immediately.
- Match RGB `(0, 0, 0)` exactly, ignore alpha, and return `(0, 0)` when absent.
  The caller chooses the run length; Spider uses five. The sentinel requires
  callers to exclude the image origin from their search area.
- Preserve padded strides, checked bounds and typed errors without allocating
  or copying image data. Keep game policy, capture and clicks in the caller.

## 0.4.0 — development

- Add const `Rank::value` and checked `Rank::from_value` with fixed A=1 through
  K=13 mapping, independent of enum discriminants, suit or game-specific rules.
- Write schema 2 with exact `card`, integer `value` and `suit` records. Derive
  the display label and value from typed identities; retain suits for all 52 cards.
- Keep strict schema-1 reading for existing rank/suit exports. Require each
  version's exact card shape and reject mixed fields, mismatched labels/values,
  non-integer/out-of-range values, unknown/duplicate keys and invalid inventories.
- Preserve card order, deal metadata and acquisition provenance on explicit
  read/write conversion. Do not overwrite or relabel accepted live originals.
- Retain fixed arrays, the 65,536-byte document cap, no-overwrite publication,
  dependency-free default features and all existing QMP/image interfaces.
- Default writer output changes from schema 1 to 2; older schema-1-only readers
  must be upgraded. No route search, game-rule execution or input authority added.

## 0.3.0 — development

- Add independent `cards`, `pyramid` and `pyramid_json` optional features while
  preserving the dependency-free default, root telemetry and existing APIs.
- Model the standard 52 rank/suit identities and an immutable original deal
  containing exactly 28 tableau cards and 24 first-draw stock cards. Reject
  duplicates and invalid logical positions without inferring missing identities.
- Validate bounded provenance and distinguish synthetic/manual fixtures from
  producer-declared verified restoration after screen recognition.
- Add schema-1 JSON slice/stream readers and a deterministic bounded byte writer.
  Enforce a 65,536-byte document cap, fixed collection sizes, strict object shapes,
  metadata bounds, supported schema/game, uniqueness and completion declarations.
- Reject unknown/duplicate fields, null evidence, positional struct arrays,
  malformed identities and trailing documents with bounded diagnostics.
- Add a labelled synthetic complete fixture and independent public-model/codec
  integration coverage. These do not establish live card recognition.
- Add optional Unix `pyramid_json::write_new` publication through a bounded private
  same-directory temporary, file sync, atomic no-clobber hard link and parent sync.
  Preserve pre-publication, uncertain-link and published-but-cleanup/durability
  failure states, original I/O causes and retained temporary paths. Refuse
  non-Unix publication instead of weakening the private-file permission contract.
- Keep capture/input, live recon, restoration verification,
  console intake and search outside this foundation. No registry publication.

## 0.2.1

- Add allocation-free `image_matching::comparison::count_rgb_changes` over
  independently checked RGBA views, with exact u64 RGB-delta counts.
- Validate equal dimensions, half-open ROIs and all exclusion rectangles;
  preserve padded strides and count overlapping masks as a union.
- Add a separate `ComparisonError` without changing existing matching APIs,
  telemetry, feature dependencies, edition or MSRV.
- Cover threshold extremes, alpha/padding, geometry failures and multi-mask
  intersections with synthetic tests and a public API example.
- Pair with qmp-qemu-socket 2.0.11 through its direct sibling path dependency
  during local development. Crate-first promotion supplies the exact consumer
  Git revision; the development path is not the release dependency. No registry
  publication is implied.

## 0.2.0 candidate 1, revision 2

- Preserve the local crate-category correction and copyright attribution.
- Bind installation to the reviewed local content at the same committed base.
- Preserve existing source permissions and restore exact local bytes and modes
  during guarded rollback. No transport, image or gameplay behaviour changes.

## 0.2.0 candidate 1

Based on `cbee27824f21d6d04d52adb49a7a0266cfc7ed19` (`0.1.0`). This candidate
is paired with the `qmp-qemu-socket` 2.0.0 milestone and remains unpublished.

- Preserve the root telemetry types and dependency-free default build.
- Add dependency-free pixel/QMP geometry with unchanged integer mapping,
  checked midpoint access and unsigned-safe rectangle membership.
- Extract Unix QMP transport behind `qmp`, replacing the game-specific key
  operation with caller-selected qcodes and configurable timing. Preserve
  acknowledgement ordering and release-only recovery without input replay.
- Add checked borrowed RGBA views and generic predicate run, count and block
  scans behind `image_matching`. Retain public owned-frame fields for application
  adapters while validating immutable views before scans.
- Add PNG-only decoding behind `png`; capture acquisition and temporary-file
  ownership remain with the application.
- Declare Rust 1.101.0 for the new milestone; verification uses the isolated
  1.101 nightly toolchain and does not establish a lower supported minimum.
- Keep game calibration, HALO anchor normalisation, title/UI policy and action
  selection outside xsar. Add focused synthetic-image and mock-socket regressions.

## 0.1.0

Initial drone telemetry enums, 3D position alias, telemetry structs, display
implementations and construction/discriminant tests.
