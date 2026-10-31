# xSAR Core Architecture — 0.9.1 host verified

## Qualified joint RGB working tree — package 0.9.1

The qualified working tree adds dependency-free
`image_matching::packed_rgb::compare_packed_rgb(before, after, tolerance, maximum_changed_percent)`.
It checks equal lengths and a valid percentage, ignores the high alpha byte of
LE RGB0, and counts only same-pixel all-RGB deltas above tolerance. The old
packed-byte API remains unchanged for quiet settling and other callers.

The qualified xSAR working tree retains 0.9.1; its added comparator remains an unreleased path-dependency change.

All sixteen library gates pass: default 14 tests, seven individual feature
checks, all-feature check, strict private rustdoc, Clippy, 303 full tests,
release and two new focused Miri checks. The real consumer 2.9.168 passes
1,669 full tests and aligned locked check/release. No library publication,
Git promotion or installation occurred. Both CPU configurations are recorded
in [the consumer verification](../qmp-qemu-socket/docs/Verification-2.9.168.md).

## Retained 0.9.1 packed-byte delivery

## 0.9.1 — safe independent packed-byte comparison

The image_matching feature exports compare_packed_bytes(before: &[u32],
after: &[u32], tolerance: u8). Equal-length words are unpacked with to_le_bytes;
each of the four byte positions is independent and changes only when its
absolute difference is strictly greater than tolerance. Absolute sums include
every delta, with maxima and changed counts per position. The operation uses
a safe plain loop without allocation, unsafe code or an internal timer.

Length mismatch and conservative accumulator-size bounds return typed errors;
empty slices produce zero statistics. The caller owns RGB/alpha interpretation.
The existing count_rgb_changes API and its different threshold semantics are
unchanged. The packed-byte-comparison example times one whole 1,519-word call
only, with preparation and output outside the timer. One observed 7.314 us is
not a performance guarantee.

The default dependency-free checks, seven individual feature checks, all-feature
check/tests, strict private rustdoc, Clippy, release and two focused Miri tests
pass: 299 tests in the full scope, 14 default tests. qmp-qemu-socket 2.9.167 is
the real qualified consumer. No registry publication or Git promotion occurred.

## Retained 0.9.0 context

## 0.9.0 — additive cooperative search control

Add `solve_with_control`, `solve_best_effort_with_control` and Pyramid
`solve_best_effort_with_cost_and_control`. Existing APIs retain never-paused behaviour.
The caller-owned checkpoint retains the same frontier and reports only intentional
suspension; each report is bounded by its measured callback span. Active elapsed time
is shared by deadlines and statistics, including cancelled outcomes and best-effort
fallback. Cancellation is checked before and after the callback. Ordinary observer,
problem and callback overhead remain counted; a blocking callback must cooperate with
cancellation. No dependency or input capability is added.

Host qualification passed 296 all-feature tests with zero failures, ignored or filtered
tests, plus default/feature checks, strict private documentation and Clippy. The native
qmp-qemu-socket 2.9.139 consumer passed its full qualification and release build.
Fresh guest acceptance remains application-owned.

## Retained 0.8.105 timing refactor

The timing refactor centralises existing QMP, TCP and search policies in [`src/parameters.rs`](src/parameters.rs), preserving values and caller overrides. Required host verification passed **289 tests** and six feature configurations. Shared QMP defaults are public; neutral feature trees remain dependency-free. See the [timing catalogue](https://github.com/xSAR-research/qmp-qemu-socket/blob/main/docs/Timing-Constants.md). The 0.8.104 qualification below is retained history.

<!-- qualification104:start -->
Version **0.8.104 is host qualified**. The additive SearchProblem action_cost hook
defaults to one, permits checked zero/non-unit costs and keeps cumulative cost
separate from immutable route depth. State dominance is checked cost/depth;
zero-cost cycles do not create equal-label histories. Exact proof concerns the
declared primary cost. Existing Pyramid APIs retain LogicalActions; the new
explicit PyramidCost::CardMatches adapter counts MATCH/KING as one and DRAW/RECYCLE
as zero. Pure tableau-removal scoring and aggregate fallback bounds remain.

Required shared tests passed **289/0/0**, matrix **14/55/25/67/57/99** (overlapping),
real app **1260/0/0**, private unchanged-source console **80/0/0**,
all zero filtered. Strict private docs/Clippy/native release, scoped formatting
and dependency-free neutral trees passed. No dependency or unsafe code was added.
Consumer calibration, UI, actual input and challenge lifetime stay in the app.
Fresh gameplay is separate. See the
[paired verification](../qmp-qemu-socket/docs/Verification-2.8.119.md).
The 0.7.103 block below is retained history.
<!-- qualification104:end -->

<!-- qualification103:start -->
Version **0.7.103 is host qualified** for additive scored fallback search. Existing
`solve` and `solve_with_options` contracts are unchanged. The new API first seeks
a full goal, then spends only the remaining aggregate budget on unpruned scored
search. Exhaustion proves maximum score and shortest equal-score cost; a bound
reports only best found. Cancellation, including a final progress callback STOP,
allocation and invariant failure expose no executable prefix.

The pure Pyramid adapter scores tableau removals only. Consumers independently
replay returned routes and own any input/completion authority. App 2.7.118 owns
native calibration and stops automatic play after an exactly verified partial
endpoint. Full shared checks passed **278/0/0**, six feature configurations passed
**14/46/25/58/57/89** (overlapping), the real app passed **1,218/0/0** and the private
unchanged-source console passed **80/0/0**, all zero filtered. Strict private docs,
Clippy, release and four dependency-free neutral feature trees passed. The original
console source and historical lock remain untouched; only the private copy updates
the xSAR lock entry for compatibility testing. Unchanged shared successes were
sealed and retained during an app-only assertion correction. Final paired
documentation/preservation audit passed: 12 documents, 309 local links and 40 review
paths, with exercised inputs preserved. Fresh app gameplay is not supplied by
host checks. Defaults, dependency graph and old APIs remain unchanged. See the app's
[verification record](../qmp-qemu-socket/docs/Verification-2.7.118.md). The 0.6.102
evidence below is retained history.
<!-- qualification103:end -->

Rust telemetry data structures with optional reusable QMP transport, pixel
geometry, PNG decoding, predicate-based image scans and pure card/deal models
with a bounded Pyramid JSON codec, optional generic bounded search and observed
execution sequencing, and a pure Pyramid planning adapter.

The existing `NodeState`, `FlightModeStatus`, `Position`,
`FlightControllerState` and `MeshTelemetry` remain at the crate root. Default
builds have no external dependencies. These types describe drone telemetry;
they do not implement a mesh protocol or state-transition engine.

| Item | Contract |
|---|---|
| License | [AGPL-3.0-or-later](LICENSE) |
| Repository | https://github.com/xsar-research/xsar |
| Contact | charlie`xsar.com.au / info`xsar.com.au |
| Edition | Rust 2024 |
| Declared minimum | Rust 1.101.0 |
| Candidate provenance | Base `cbee27824f21d6d04d52adb49a7a0266cfc7ed19` |

Version **0.6.102 is host verified** for the approved additive search-options
milestone: 266 tests passed, zero failed/ignored, all required checks/release
and six feature configurations passed, and the real app 2.6.108 consumer passed
1,043 tests plus its required checks. A private frozen-console-copy smoke passed
80 tests; the original console was not built or changed. `SearchOptions` supplies a positive heuristic weight;
`solve_with_options` adds borrowed synchronous progress while existing `solve`
signatures retain exact behaviour. App 2.6.108 consumes the addition directly;
accepted console 0.8.4 remains frozen. No feature, dependency, MSRV, edition or
registry publication change is included. One agreed minor/build is earned after the complete required pass. The application's
[verification record](../qmp-qemu-socket/docs/Verification-2.6.108.md) records
paired repository results and clearly separates host proof from live acceptance.
Native Continuous remains unavailable in the consumer without positive native
progress/standard-target evidence; this crate does not infer or own that policy.

Previous host verification: Version **0.5.101 is host verified** for the approved reusable planning/execution
milestone. Complete required checks passed, applying one agreed minor increase
from 0.4.100 and exactly one build increment. The build counter is independent
of the application's counter and never resets. The
[current verification record](docs/Verification-0.5.101.md) records 256 passing
all-feature tests, the feature matrix and real application-consumer verification.
Live guest acceptance and publication remain separate decisions.

The declared minimum remains Rust 1.101.0. The 0.2.1 RGB slice is promoted at
`987856a0b5cb95aec2788551e050e18f7469b1ef` and consumed by the promoted
qmp-qemu-socket 2.0.11 application. The 0.4.0 model/JSON extension is local
development work, not a promoted or registry-published release.
Use an explicitly selected installed 1.101 nightly when the host default is
older than this MSRV; successful nightly checks do not establish lower-toolchain
support. No toolchain file, global default or registry publication is changed.

Copyright 2026 xSAR Research.

## Features and modules

| Feature | Modules | Platform and dependency boundary |
|---|---|---|
| Default (`[]`) | Existing telemetry and `geometry` | `std`, no external dependencies |
| `search` | `search` | Portable `std`; generic unit-cost A*/Dijkstra with optional weighted guidance, no external dependencies |
| `execution` | `execution` | Portable `std`; generic finite observed-operation sequencing, no external dependencies |
| `qmp` | `qmp` | Unix sockets; optional `serde_json` and `thiserror` |
| `image_matching` | `image_matching` | Portable `std`; no external dependencies |
| `png` | `capture`, also `image_matching` | Optional PNG-only `image` and `thiserror` |
| `cards` | `games::cards` | Portable `std`; no external dependencies |
| `pyramid` | `games::pyramid`, also `games::cards` | Enables `cards`; no external dependencies |
| `pyramid_rules` | `games::pyramid::{planning,rules,replay}` | Enables `pyramid` and existing optional `thiserror`; no JSON, capture or input dependencies |
| `pyramid_search` | `games::pyramid::search` and the rules modules | Enables `pyramid_rules` and `search`; no JSON, capture or input dependencies |
| `pyramid_json` | `pyramid_json`, also both model modules | Optional `serde` with std/derive and existing `serde_json`; no QMP/image/GUI dependencies |
| `pixel_format` | `pixel_format` | Portable byte-layout conversion; no external dependencies |
| `tcp_capture` | `tcp`, also `pixel_format` | Bounded standard-library TCP capture and shared protocol; no QMP/PNG/GUI dependencies |

The TCP client supports version-3 BGR8 and bounded persistent sessions. The
earlier candidate pairing used screen-grab server 0.1.2 and qmp-qemu-socket
candidate.13.
See the [TCP protocol and client contract](docs/tcp-capture-protocol.md).
Version 2 remains an explicit comparison mode; new clients default to version 3.
The older promotion records below describe their own historical revisions.

The flags are additive and can be enabled together. On non-Unix targets the
`qmp` module is unavailable; the other modules remain usable. The crate does not
claim `no_std` support. GUI/windowing dependencies, calibrated card geometry,
recognition, live action selection and input authority belong in the consumer.

The promoted qmp-qemu-socket 2.0.11 application selects the RGB release:

```toml
xsar = { git = "https://github.com/xsar-research/xsar", rev = "987856a0b5cb95aec2788551e050e18f7469b1ef", default-features = false, features = ["qmp", "png", "image_matching"] }
```

That revision does not include the new model/JSON or planning APIs. The initial
qmp-qemu-socket 2.5.102 host-verified consumer used this checkout through
`../xsar`, selecting `pyramid_search` and `execution` alongside its existing
transport, image and JSON features. Those historical consumer checks passed. Current app 2.6.108 uses the same
relative path plus the additive options/progress API; new paired host checks
passed in complete attempt 0002. The path does not identify a Git revision pin. The accepted console remains frozen at
0.8.4, with no migration or console dependency added to the application.
Promote each accepted crate slice first, then pin consumers to its actual Git
revision and revalidate with locked dependencies. Do not substitute a host-only
path or unavailable registry version in promotion manifests. Registry
publication remains a separate explicit decision.

## Pure cards and complete Pyramid deals

`games::cards::{Rank, Suit, Card}` describe exactly the standard 52 identities.
Their canonical names are A, 2..10, J, Q, K and clubs, diamonds, hearts, spades.
`Rank::value()` is the fixed Ace-low integer 1..13; `Rank::from_value(u8)`
returns `None` outside that domain. Both are const functions and do not use
enum discriminants. The mapping is not a game-specific score or move-legality
test. Suits remain part of identity even when a particular game's matching rule
ignores them; consumers must not merge four equal-rank cards into one identity.
Unknown/ambiguous observations are not valid cards. They stay with the screen
reader and must not be silently filled by elimination.

`games::pyramid::CompleteDeal::try_new` takes fixed `[Card; 28]` and `[Card; 24]`
arrays and checked `DealMetadata`. The constructor rejects duplicate identities
within or across both arrays using fixed 52-slot storage. All 52 distinct values
in this closed domain establish a complete standard deck. No model fields or
array accessors allow unchecked mutation after construction.

Tableau order is top-to-bottom, left-to-right: rows 0..6 and columns 0..row.
`card_at(row, column)` checks those bounds before calculating the array index.
Stock order is first-draw order; ordinal zero is the initial visible LEFT card
before any advance. These are original positions, not a live stock cursor or
the application's bottom-up CV target indices. The base `pyramid` feature remains
model-only; rules and search require their separate opt-in features.

Metadata validates a nonempty ASCII deal token (at most 64 bytes), producer
(64 UTF-8 bytes), source (256 UTF-8 bytes) and optional evidence token (128 ASCII
bytes). Tokens allow letters, digits, hyphen and underscore; text rejects control
characters. Source/evidence labels are never opened as paths. Synthetic/manual
fixtures require `NotApplicable` restoration; screen recognition requires the
producer's `Verified` declaration. A valid declaration is not proof of actual
recognition, restoration, authenticity or input authority.

## Generic search and finite observed execution

`search::SearchProblem` supplies a copied/hashable state, copied action, finite
successor iterator, initial state and goal. Optional admissible guidance and a
sound impossible-state predicate support A*. `search::solve` returns typed
`SearchOutcome<Action>`: solved, proved unsolvable or stopped without proof.
Dijkstra bypasses both optional guidance callbacks. Actions have unit cost;
the original `solve` entry point retains exact ordering. The additive
`solve_with_options` accepts `SearchOptions` with positive `heuristic_weight`;
weight one is exact A*, larger weights may return longer routes. Dijkstra
ignores the weight and both optional guidance callbacks. No universal speedup
is promised. Deterministic tie ordering, immutable
predecessor history, reopened states, fallible collection growth and cooperative
cancellation remain engine responsibilities.

Defaults are 2,000,000 retained records, 2,000,000 expansions, 512 MiB of estimated
collection storage and 120 seconds. The memory estimate accounts for actual
generic state/action sizes; it does not measure RSS, allocator metadata, the
caller's problem or its successor iterator. Callbacks must terminate and keep
their own work bounded. A synchronous callback cannot be interrupted mid-call.

`execution::Sequencer<Operation, Epoch>` consumes a finite sequence of typed
application operations. Single-use tickets bind precondition observations,
dispatch receipts, waits and effect observations to the correct run and phase.
It reserves each whole logical action's gesture budget before dispatch, returns
each operation once and commits an action only after the caller verifies its
effect. Changed authority, STOP, failed or uncertain delivery, stale replies,
failed evidence, exhausted budgets and deadlines terminate the run. There is no
retry or resume API. Deadlines are cooperative at API boundaries.

The sequencer performs no I/O and recognises no game. The application owns fresh
evidence, native coordinates, transport probes, STOP/context checks immediately
before every gesture, and the meaning of acknowledgement versus effect. These
generic modules have non-card consumer tests; no other game's rules are added.
The additional public consumer in `tests/search_execution.rs` connects both:
generic search finds SetSeven/Apply, then the neutral sequencer verifies those
two actions through Focus/WriteSeven/PressApply. It requires only `search` and
`execution`, without cards, JSON, images or transport. It passed both the
all-feature suite and the independent `search,execution` feature configuration.

`SearchProgress` contains actual cumulative `SearchStats` plus `finished`.
The optional borrowed observer receives initial, periodic (at most once per
200 ms), and one final notification matching a normally returned outcome.
The final notification bypasses throttling. Callbacks must return promptly;
cancellation is checked around them but cannot interrupt a callback. Search and
non-final observer time count against the deadline. A caller panic propagates
without a normal final-notification guarantee. No thread, queue, I/O or domain
policy is introduced. `SearchOptions::default()` preserves exact A*, and old
public callers/feature contracts remain compatible.

## Pure Pyramid planning adapter

With `pyramid_rules`, `games::pyramid::planning` owns checked `TableauLocation`,
`Location`, `RecordedAction`, compact `SearchState` and replay/error types.
Public mappings use fixed one-based rows/columns; `CompleteDeal::card_at` keeps
its existing explicitly zero-based boundary. Card identities remain solely in
the immutable complete deal. Presence state never copies or invents identities.

`games::pyramid::rules` preserves the accepted two-recycle profile: exposed
sum-13 pairs, single Kings, both visible DL/DR tops playable, preserved remaining
stock encounter order, and tableau-clear goal. `replay::replay` independently
simulates explicit stock/waste piles without calling solver transitions or
pruning. It accepts at most 4,096 logical actions. `search::solve` in that Pyramid
namespace borrows the deal and adapts these rules to the generic search engine.
Pyramid's fixed 64-successor bound is confined to this adapter. Its additive
`search::solve_with_options` forwards the same options/observer contract without
changing rules, action costs or independent replay responsibility.

The upstream 15/44 costs and accepted separate v6 48-action/v7 49-action routes
are retained regression inputs, with independent replay and the
[MIT provenance notice](third_party/README.md). Their historical manual success
does not authenticate a current live board. JSON fixture tests additionally
select `pyramid_json`; pure rules and mappings work without that codec.

The active application consumes xSAR directly for this planning and sequencing.
It alone owns the same-session restored-acquisition receipt, exact export-byte
binding, calibrated observations and guarded QMP execution. The accepted console
0.8.4 is byte-unchanged; its earlier delivery evidence remains separate.

## Pyramid JSON schemas 1 and 2

The optional `pyramid_json` feature exposes `from_slice`, `read_from`, `to_vec`
and `write_new`. The [complete synthetic fixture](tests/fixtures/pyramid-synthetic-v1.json)
is retained as a legacy read fixture, not evidence of a recognised live deal.
The [schema-2 fixture](tests/fixtures/pyramid-synthetic-v2.json) expresses the
same arrangement with explicit numeric values. Writers emit schema 2 only.
It enumerates suits clubs, diamonds, hearts, spades and ranks Ace through King:
the first 28 cards form the tableau and the remaining 24 form the stock.

| Required root field | Contract |
| --- | --- |
| `schema_version` | Integer 1 or 2 on input; writers emit 2 |
| `game` | `pyramid` only |
| `deal_id` | Checked ASCII label |
| `complete` | Boolean true; partial inventories are refused |
| `restoration` | `verified` or `not_applicable`, compatible with provenance |
| `provenance` | Object with `kind`, `producer`, `source`, optional `evidence_id` |
| `tableau` | Exactly 28 version-specific card objects in canonical original order |
| `stock` | Exactly 24 version-specific card objects in first-draw order |

Provenance kinds are `synthetic_fixture`, `manual_fixture` and
`screen_recognition`. Schema 1 cards have exactly `rank` and `suit` string fields.
Schema 2 cards have exactly `card`, `value` and `suit`:

```json
{ "card": "K of spades", "value": 13, "suit": "spades" }
```

`value` is an integer from 1 through 13: A=1, 2..10 unchanged, J=11, Q=12,
K=13. Suit is one of the four canonical lower-case names. These determine the
typed rank/suit identity; `card` must equal its exact `Display` representation
(at most 14 ASCII bytes, for example `10 of diamonds`). Labels are not parsed
as an independent source of truth. Mismatched rank/value/suit labels refuse.
`rank` is forbidden in schema 2; `card` and `value` are forbidden in schema 1.
Missing version-specific fields, explicit null, floats and numeric strings
are not compatibility defaults. Object-key order, including the root version's
position, does not affect validation.

Reading schema 1 into a model and writing it produces schema 2 without changing
identities, order, deal metadata or acquisition provenance. This is an explicit
representation upgrade, not a new recognition or restoration claim. Preserve
accepted original files and use a separate no-overwrite output path. An older
schema-1-only reader cannot read the new writer's output and must be upgraded.

Unknown fields, duplicate keys, positional arrays in place of objects, null
fields, unsupported versions/games, alternate identity spellings, missing or
extra cards and trailing non-whitespace are rejected. `evidence_id` may be
omitted but cannot be null. Object-key order does not affect semantic identity.
Native coordinates, confidence, images and arbitrary metadata are excluded;
retained external diagnostics may be referenced by the opaque evidence token.

The whole encoded document, including whitespace and newline, is limited to
65,536 bytes. Slice input is checked before parsing; streams read at most 65,537
bytes to detect excess, independent of file metadata. Fixed arrays avoid
attacker-directed collection lengths. String decoding may use scratch storage
up to the document cap before field checks. Recursion protection remains enabled.
No exact RSS, allocation-failure recovery or blocking-reader deadline is promised.

`to_vec` emits two-space-indented UTF-8 JSON with stable field/card order and a
trailing newline through a byte-limited buffer. It does not preserve input
whitespace or partially return a failed encoding. Errors distinguish input I/O,
size limits, malformed wire input, unsupported schema/game and invalid deal data;
diagnostics retain logical locations without echoing an entire input document.

### No-clobber filesystem publication

`pyramid_json::write_new(path, &deal) -> Result<(), PublishError>` adds Unix-only
publication without changing the byte/stream APIs or adding dependencies. It
serialises the validated immutable deal before filesystem access, then:

1. Opens the existing parent directory for its later durability sync.
2. Reserves a same-directory private temporary using atomic `create_new`, mode
   `0600` further restricted by umask, and at most 64 candidate-name attempts.
3. Writes the whole bounded document and calls the temporary file's `sync_all`.
4. Creates the destination atomically with `hard_link`, never replacing a file,
   directory or symlink, including a dangling symlink. There is no rename fallback.
5. Removes only its own temporary link and attempts parent-directory `sync_all`.

Check `PublishError::publication_status()` rather than assuming every error means
nothing was published:

| Status | Meaning and caller action |
| --- | --- |
| `NotPublished` | This call did not attempt a destination link; inspect the primary stage and any cleanup failure. An older destination may still exist. |
| `Uncertain` | The destination-link call returned an error. Even `AlreadyExists` is conservative because remote filesystem acknowledgement can be ambiguous. Inspect before retrying; never overwrite. |
| `Published` | Linking succeeded, but cleanup and/or parent-directory sync failed. The complete file was published; do not replay publication as if it had failed. |

`Ok(())` confirms both sync requests, link creation and temporary removal succeeded;
it is not an unconditional power-loss guarantee beyond the filesystem/device.
`PublishError::Io` retains the operation stage, original cause and independent
cleanup error/path. `PublishError::Published` retains cleanup and directory-sync
failures separately. Top-level display omits paths and original I/O text; public
fields, error chains and Debug need caller-controlled presentation.

The parent hierarchy must be trusted and stable, with enough permission to open
and sync it. Intermediate symlinks resolve normally. Relative paths additionally
require a stable process working directory. This standard-library, path-based API
does not prevent hostile ancestor/private-name replacement. Temporary names are
predictable but reserved exclusively; stale files are never reclaimed. Hard-link
support is required. No directories are created and no destination is deleted.
Non-Unix publication refuses with `UnsupportedPlatform` before filesystem access;
the portable models and byte/stream codec remain usable.

Normal errors attempt cleanup and report retention; unwinding is best-effort.
Process death or power loss can leave a private temporary. Byte storage remains
bounded by 65,536 bytes; path storage follows the supplied path length. No hard
filesystem I/O deadline or exact RSS bound is promised.

No file-save dialog, reconnaissance, Undo All operation, restoration verifier or
acquired-deal workflow is implemented here. Producer metadata is a declaration,
not live restoration proof. Callers keep partial diagnostics separate and grant
export authority only after their own complete inventory/restoration checks.

## Checked geometry

`geometry` exports `PixelPoint`, half-open `PixelRect`, `QmpPoint`, inclusive
`QmpRect`, `GeometryError` and the three pixel-to-QMP conversion functions.
The existing drone `Position = (f64, f64, f64)` remains distinct.

Conversions retain the integer mapping `pixel * 32767 / extent`, using widened
arithmetic and truncating division. They reject zero extents and coordinates
outside the declared frame. Rectangle conversion uses its final included pixel
(`right - 1`, `bottom - 1`); it does not substitute `extent - 1` in the formula.

`PixelRect::try_centre` checks midpoint arithmetic and signed representability.
The existing `centre` convenience method preserves calibrated constant use and
panics when that midpoint cannot be represented; use `try_centre` on untrusted
geometry. `contains` rejects negative points and overflowing rectangle edges.

## QMP input and capture contract

`QmpClient::connect` retains default timing. `connect_with_config` accepts
`QmpConfig` fields `io_timeout`, `screendump_timeout`,
`pointer_settle_delay`, `mouse_hold` and `key_hold`. Default durations are
750 ms, 5 s, 100 ms, 50 ms and 20 ms respectively. Socket timeouts must be
non-zero; hold and settle durations may be zero.

Status/pointer probes, `move_pointer`, `click`, `click_with_hold` and
`screendump_png` retain the controller's exchange order. `press_key(qcode,
hold)` and `press_key_default(qcode)` accept the caller's QEMU key identifier;
there is no game-specific Draw operation in this crate.

Requests carry checked numeric IDs. Asynchronous events do not satisfy requests,
and paired input responses may arrive in either order. Pointer movement requires
its acknowledgement before a click begins. Key/button release is transmitted
before waiting for the down acknowledgement. An uncertain input is never replayed:
recovery attempts only release on the old stream, closes it, then confirms release
once on a fresh connection with the same configuration. A recovery failure retains
both the original causal error and the recovery error.

Timeouts bound individual socket I/O operations, not an overall command deadline
or connection establishment. Continuous events can prolong response collection;
packet length is not independently limited. Acknowledgement proves command
acceptance, not a visible effect or permission to take another action. The caller
retains cancellation, VM/pointer preconditions and effect verification.

`screendump_png` requires an absolute UTF-8 path without NUL bytes. QEMU writes
that file; the caller must arrange shared filesystem access and file cleanup.
The method sends no pointer or keyboard input. `capture::decode_png` decodes
caller-supplied bytes into tightly packed RGBA storage and does not acquire a
framebuffer or eliminate screendump's file operation.

## Image scans and evidence

`CapturedFrame` retains public fields for existing adapters, so direct construction
can create invalid metadata. `as_view` / `FrameView::from_rgba` validate stride,
checked host arithmetic and storage before immutable access. The view supports row
padding; `pixel_rgb` checks coordinates and byte offsets. RGB predicates ignore
alpha while the backing frame retains alpha bytes.

The reusable scans accept half-open full-frame ROIs and caller predicates:

- `find_horizontal_run` returns the first qualifying full run in top-to-bottom,
  then left-to-right order. Minimum length and optional `RunStartConstraint`
  tolerance are inclusive. The returned anchor is measured and is never moved
  to a calibrated anchor.
- `find_solid_block` returns the first leftmost consecutive block of columns
  matching across the entire supplied probe height. Partial columns and isolated
  bright pixels cannot qualify.
- `count_pixels` returns widened exact `PixelCounts { matching, total }`.
  `meets_fraction_per_mille` compares the ratio without rounding;
  `fraction_per_mille` rounds down only for presentation.
- `matches_palette` compares each RGB channel with caller-supplied palette entries
  and an inclusive tolerance. It embeds no application colours.

`Result<Option<Hit>, MatchError>` distinguishes no match from invalid frame,
bounds, zero minima or unrepresentable signed anchors. Counting returns measured
evidence without scene classifications. These are colour/run/block primitives,
not scale/rotation-aware reference-image template search. HALO palettes, probe
lines, anchor-to-click invariants, per-game ordering and completion decisions stay
with application wrappers.

## Two-frame RGB comparison

`image_matching::comparison::count_rgb_changes(before, after, bounds,
exclusions, threshold)` accepts two independently checked `FrameView`s and
returns `Result<u64, ComparisonError>`. Both frames must have equal dimensions;
their valid strides and padding may differ. A pixel counts once if any RGB
channel's absolute delta is at least the supplied threshold. Alpha and padding
are ignored. Threshold zero counts every unexcluded pixel, including unchanged
pixels; 255 requires an extreme channel difference.

The ROI and every exclusion must be non-empty, checked half-open rectangles
wholly inside the frame. Exclusions contribute only their intersection with
the ROI; disjoint masks are valid, and overlapping or duplicate masks form a
union. Every mask is checked before scanning, even after a full-coverage mask.
Valid no-change and fully excluded regions return `Ok(0)`, never an error.
Invalid input is an error, never a zero fallback. `ComparisonError` reports
dimension pairs or the underlying `MatchError`, including the failing exclusion
index. Existing `MatchError` variants and single-frame APIs remain unchanged.

The primitive allocates no memory and performs no I/O or early effect-floor
exit. With ROI area A and E exclusions, worst-case work is O(E + A * (E + 1))
with constant auxiliary storage. The exact u64 count is bounded by the product
of the ROI's u32 dimensions. Checked view construction separately bounds host
index arithmetic and storage; no mask-count limit or execution deadline is
imposed. Callers retain scene recognition, cursor geometry, material-effect
floors, cancellation and all input/completion authority.

The real application consumers are its worker effect-count adapter and Pyramid
pile-interior adapter. Game fixtures and decisions remain in the application;
crate tests use synthetic image evidence only.

## TCP screen capture

Version 3 is the current default; see the [current protocol contract](docs/tcp-capture-protocol.md).
The wire layout below documents the explicit legacy version-2 comparison mode.

The optional `tcp_capture` feature provides the synchronous `tcp::ScreenCaptureClient`
and shared `tcp::protocol` used by the Windows screen-grab server on port 9999.
Protocol version 2 sends the five request bytes `42 02 01 00 00`. Each connection
returns one frame with a packed, 14-byte little-endian header:

| Byte offsets | Field | Type |
| --- | --- | --- |
| 0..4 | Width in pixels | `u32` |
| 4..8 | Height in pixels | `u32` |
| 8..10 | Stride in bytes per row | `u16` |
| 10..14 | Frame number | `u32` |

The payload is top-down BGRA: B, G, R, A bytes, with alpha set to 255 by the
server. The client swaps B and R to produce RGBA. Both layouts use four bytes
per pixel, so `stride = width * 4`: 1920 pixels need 7680 bytes per row.
Although `u16` reaches 65535, complete four-byte pixels limit packed stride to
65532 bytes, or 16383 pixels wide; 16384 pixels require 65536 bytes and refuse.
The independent full-frame limit remains 8 MiB. Internal buffer indexing uses
`usize`, widened from the validated wire stride.

Signed producer values are checked before narrowing: negative, zero, oversized
and misaligned strides refuse. Unsigned wire bytes cannot identify every value
that a sender incorrectly cast from a negative integer. The receiver checks
nonzero dimensions, exact packed stride and checked frame size before allocation.

This changes the earlier request/header layouts. Rebuild and deploy both client
and server together; version-1 requests are refused. The controller's use of
QMP or TCP remains application policy. The socket tests use a local mock server,
including full-HD captures, and do not establish live Windows GDI behavior.

## Validation

Run checks separately from the crate root using the intended toolchain:

```text
cargo check --locked
cargo test --locked
cargo test --locked --no-default-features --features cards
cargo test --locked --no-default-features --features pyramid
cargo test --locked --no-default-features --features pyramid_json
cargo test --locked --no-default-features --features image_matching
cargo test --locked --no-default-features --features qmp
cargo test --locked --all-features
cargo doc --locked --all-features --no-deps --document-private-items
cargo clippy --locked --all-features --all-targets
cargo build --release --locked --all-features
```

Focused tests cover geometry vectors and bounds, byte-exact PNG decoding,
invalid layouts, deterministic run ordering, threshold edges, full-column blocks,
predicate counts and mock Unix QMP ordering/recovery. Tests use synthetic frames,
an embedded minimal PNG and mock sockets; they do not establish live guest
compatibility or successful gameplay.
