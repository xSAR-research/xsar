# xSAR Core Architecture — 0.2.0 candidate 1, revision 2

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

The declared minimum aligns with the paired controller milestone. Candidate
validation uses an isolated 1.101 nightly toolchain; successful checks do not
establish support for lower toolchains. See the delivery evidence for the exact
compiler identity and checks that ran. This directory is an unpublished
candidate, not the currently published crates.io `0.1.0` release.

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

For a local paired checkout, Cargo can patch the crate dependency without
changing the application manifest to a machine-specific permanent path:

```text
cargo test --manifest-path /absolute/path/to/controller/Cargo.toml \
  --config 'patch."https://github.com/xsar-research/xsar".xsar.path="/absolute/path/to/xsar"'
```

The application release must ultimately pin the exact promoted xsar commit.
The candidate pairing validates these local files; it does not establish an
upstream `0.2.0` release or promotion.

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
