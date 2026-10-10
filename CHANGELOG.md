# Changelog

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
