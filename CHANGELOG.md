# Changelog

## Unreleased

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
