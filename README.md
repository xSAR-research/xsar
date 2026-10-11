# xSAR Core Architecture — 0.4.0 development

Rust telemetry data structures with optional reusable QMP transport, pixel
geometry, PNG decoding, predicate-based image scans and pure card/deal models
with a bounded Pyramid JSON codec.

The existing `NodeState`, `FlightModeStatus`, `Position`,
`FlightControllerState` and `MeshTelemetry` remain at the crate root. Default
builds have no external dependencies. These types describe drone telemetry;
they do not implement a mesh protocol or state-transition engine.

| Item | Contract |
|---|---|
| License | [AGPL-3.0-or-later](LICENSE) |
| Repository | https://github.com/xsar-research/xsar |
| Contact | charlie@xsar.com.au / info@xsar.com.au |
| Edition | Rust 2021 |
| Declared minimum | Rust 1.101.0 |
| Candidate provenance | Base `cbee27824f21d6d04d52adb49a7a0266cfc7ed19` |

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
| `qmp` | `qmp` | Unix sockets; optional `serde_json` and `thiserror` |
| `image_matching` | `image_matching` | Portable `std`; no external dependencies |
| `png` | `capture`, also `image_matching` | Optional PNG-only `image` and `thiserror` |
| `cards` | `games::cards` | Portable `std`; no external dependencies |
| `pyramid` | `games::pyramid`, also `games::cards` | Enables `cards`; no external dependencies |
| `pyramid_json` | `pyramid_json`, also both model modules | Optional `serde` with std/derive and existing `serde_json`; no QMP/image/GUI dependencies |

The flags are additive and can be enabled together. On non-Unix targets the
`qmp` module is unavailable; the other modules remain usable. The crate does not
claim `no_std` support. GUI/windowing dependencies, calibrated card geometry,
recognition, live action selection and input authority belong in the consumer.

The promoted qmp-qemu-socket 2.0.11 application selects the RGB release:

```toml
xsar = { git = "https://github.com/xsar-research/xsar", rev = "987856a0b5cb95aec2788551e050e18f7469b1ef", default-features = false, features = ["qmp", "png", "image_matching"] }
```

That revision does not include the new model/JSON APIs. For paired foundation
development, use an isolated temporary path dependency outside promotion files.
The initial console consumer is separate work, not implemented by this crate.
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
the application's bottom-up CV target indices. No solver or move rules exist.

Metadata validates a nonempty ASCII deal token (at most 64 bytes), producer
(64 UTF-8 bytes), source (256 UTF-8 bytes) and optional evidence token (128 ASCII
bytes). Tokens allow letters, digits, hyphen and underscore; text rejects control
characters. Source/evidence labels are never opened as paths. Synthetic/manual
fixtures require `NotApplicable` restoration; screen recognition requires the
producer's `Verified` declaration. A valid declaration is not proof of actual
recognition, restoration, authenticity or input authority.

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
