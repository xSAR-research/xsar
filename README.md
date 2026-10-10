# xSAR Core Architecture — 0.2.1

Rust telemetry data structures with optional reusable QMP transport, pixel
geometry, PNG decoding and predicate-based image scans.

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

The declared minimum remains Rust 1.101.0. The two-frame comparison slice is
paired with the controller's 2.0.11 development version and is not yet promoted.
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

The flags are additive and can be enabled together. On non-Unix targets the
`qmp` module is unavailable; the other modules remain usable. The crate does not
claim `no_std` support. GUI/windowing dependencies, game names, calibrated card
geometry and action-selection rules belong in the consuming application.

For local development, qmp-qemu-socket 2.0.11 selects this sibling source
checkout directly in its manifest:

```toml
xsar = { path = "../xsar", default-features = false, features = ["qmp", "png", "image_matching"] }
```

Ordinary Cargo commands build the actual application and crate checkouts; no
temporary source override, copied workspace, Git push or registry publication
is required for development. The initial isolated pairing remains historical
validation evidence, not a prerequisite. After any targeted dependency
resolution, validate with `--locked` and record Cargo's selected source.

The application release must ultimately replace this development path with the
exact promoted xSAR Git revision. Promote xSAR first, update the application
manifest and lockfile without unrelated upgrades, then revalidate and obtain
consumer acceptance. Do not commit the local development path as the release
dependency. Registry publication remains a separate explicit decision.

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
