# Milestone Tracker & Baseline Receipt — xsar

## Active receipt — 0.11.1

Adds `QmpClient::drag`: checked source/destination mapping, source-movement acknowledgement, guard checks around button/held-movement/release stages and release before waiting for gesture replies. Distinct pixels mapping to the same QMP point refuse before input. Every error after down may have been sent performs existing old-stream/fresh-stream release-only recovery and invalidates the original client; it never repeats down or destination movement. Release may itself affect the guest; no rollback or semantic completion is claimed. Application policy owns evidence, STOP and effect verification.

All 38 paired source commands passed, including the existing optional-feature/default matrix and six library/controller gates: **323 library, 1,789 controller, 80 console** full tests, zero failed/ignored/filtered. Four new local QMP drag tests cover mapped aliases, source ACK ordering, release-before-ACK collection and release-only cancellation/error recovery. Seven aligned commands passed for app3.1.2/xSAR0.11.1. Defaults, existing transport methods, search/rules and dependency versions remain unchanged. No new Miri or fresh guest acceptance is claimed. See [paired verification](../../qmp-qemu-socket/docs/Verification-3.1.2.md).

Baseline_HEAD_SHA: `068105b322830cc254f36a488bb8b370b5213097`. Baseline_PARENT_SHA: `c4a9b839ece3668b8d46ca00676ae05c34a97943`. History/index preserved; qualified path dependency working tree remains dirty. Previous receipts below are historical.

## Active 0.11.0 — complete offline Free Cell search

Baseline_HEAD_SHA: 068105b322830cc254f36a488bb8b370b5213097
Baseline_PARENT_SHA: c4a9b839ece3668b8d46ca00676ae05c34a97943

Adds optional `freecell_rules` and `freecell_search` for complete offline routes with irreversible foundations, exact canonical equality, cached column hashes, recorded Horne transfers and independent physical replay. Adds `search::solve_guided_unlimited_with_control` without changing existing bounded exact/best-effort signatures or policies. Its ordering guidance makes no minimum-cost claim. Default features remain dependency-free; no capture/input authority or new dependency is introduced.

Qualification passed 38 source commands: xSAR dependency-free defaults and eleven isolated optional features, both owning six-gate suites, and the console consumer check/tests. Full tests passed: xSAR 319, controller 1,761, console 80, with zero failures or ignored tests. The unchanged library's 30 passing rows are retained from the first source attempt; the app's narrow Clippy correction is followed by fresh six-gate qualification. The original failure and preimages remain preserved.

Approved five-file version alignment to app 3.1.0/xSAR 0.11.0 passed library/app/console locked checks, console tests, both release builds and direct help. All 1,009 compiler inputs are sealed; 1,004 are identical across version alignment. Console package version remains 0.8.5. No new Miri or fresh guest/GUI acceptance is claimed.

The supplied exported deal `freecell-1251159-1.json` produced 42 deliberate moves + 47 automatic transfers = 89 transfers, independently replayed to all 52 foundation cards. The measured focused run expanded 195,574 states, retained 291,241 states, reached a peak frontier of 76,935 and took 8.55154759 seconds of active search. This is a measured run for this deal, not a general performance guarantee.

Consumer application 3.1.0 uses the local ../xsar path with approved uncommitted changes; console 0.8.5 retains its own version. No Git or registry publication occurred. Historical RGB/Miri evidence below is retained with its original scope. Paired evidence is recorded by the controller in docs/Verification-3.1.0.md and case freecell-search-310-9vmnzf4i.

## Retained 0.10.0 — Free Cell model and JSON

Baseline_HEAD_SHA: `c4a9b839ece3668b8d46ca00676ae05c34a97943`. Parent_HEAD_SHA: `d20dca8ad2320dcbbf869e22b35a2583317d4569`. Controller consumer 3.0.0; console 0.8.5 retains its own version and now resolves xSAR 0.10.0.

Twenty original source gates/311 full tests, independently gated model/JSON tests and existing Pyramid publication regression tests passed. Approved five-file version alignment preserves 982 of 987 inputs; locked library/app/console checks, console tests and final controller release/help passed. Original source receipts and the failed stale-console-lock attempt remain distinct historical records.

No search implementation, new RGB/Miri evidence, guest/GUI action or Git publication is claimed. Controller native RECON acceptance remains pending. [Paired verification](https://github.com/xSAR-research/qmp-qemu-socket/blob/5fa150a610bbbd2b6718bf87b6730c0b852b41e2/docs/Verification-3.0.0.md).

## Retained 168 consumer receipt — package 0.9.1

Baseline_HEAD_SHA: d20dca8ad2320dcbbf869e22b35a2583317d4569
Parent: e50f7b3222fbd803d9eeecd8a2a37ba6fec856ca

Use the exact [consumer handoff](../../qmp-qemu-socket/docs/handoff.md)
Destination Sentinel for both qualified dirty trees. It seals 920 paired
inputs including all 56 library inputs, four repository histories/indices,
complete porcelain, documents and protected executable. A path is not a Git pin.

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
in [the consumer verification](../../qmp-qemu-socket/docs/Verification-2.9.168.md).

## Retained 0.9.1 receipt — historical

## Retained paired 0.9.1 receipt — 29 October 2026

Use the current exact Destination Sentinel in
[the qualified consumer handoff](../../qmp-qemu-socket/docs/handoff.md).
It binds both dirty working trees, all four repository histories/indices,
complete porcelain, the qualified source and protected consumer. A match
retains these host checks as PASSED (Cached). Do not rerun the superseded
pre-promotion sentinel preserved in the historical section below.

| Field | Current receipt |
| --- | --- |
| Package | xSAR 0.9.1; independent patch approved in m02281 |
| Baseline_HEAD_SHA | d20dca8ad2320dcbbf869e22b35a2583317d4569 |
| Baseline parent | e50f7b3222fbd803d9eeecd8a2a37ba6fec856ca |
| Porcelain and index | Exact destination-state.json; original index unchanged |
| Source scope | Safe packed-byte comparator, registration, example and own version/docs |
| Source inputs | 54 xSAR inputs within 905 paired sealed inputs |
| Verification | 16 gates pass: defaults, seven features, all features, strict docs, Clippy, 299 full tests, release and two new Miri tests |
| Default tests | 14 pass; no default dependency added |
| Consumer | qmp-qemu-socket 2.9.167 passes 1,660 full tests and aligned locked check/release |
| Alignment | xSAR manifest/lock first, then consumer own manifest/lock and xSAR lock entry |
| Promotion | No staging, commit, push, registry publication or install |

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

## Historical 0.9.0 receipt — superseded, do not execute its sentinel

The following is preserved historical evidence, not the current gate contract.

# Milestone Tracker & Baseline Receipt — `xsar`

This file (`docs/milestones.md`) is the authoritative repository-local state receipt for the `Session-Start MCP, Memory & Baseline Gate` (`~/.codex/AGENTS.md`) and `xsar/AGENTS.md`.

**Session-Start Evaluation Rule:**
1. Run the consumer's exact **Destination Sentinel** for the approved paired promotion, or the embedded **Pre-Promotion Baseline Sentinel Check** below before promotion, via `ctx_execute`. Inspect `git rev-parse HEAD` and `git status --porcelain`.
2. Treat baseline compilation, feature-matrix checks, and the `296`-test suite as **PASSED (Cached)** when either:
   - **Pre-Promotion Qualified Working Tree:** `HEAD` is `e50f7b3222fbd803d9eeecd8a2a37ba6fec856ca` with the preserved `0.9.0` dirty baseline (`9`–`11` porcelain lines including `docs/milestones.md` / `docs/handoff.md`) and the sentinel outputs `HANDOFF VERIFIED`, **OR**
   - **Post-Promotion Clean Tree:** `git status --porcelain` is empty (`0` lines) and `Baseline_HEAD_SHA` matches `HEAD` or `HEAD~1`, **OR**
   - **Approved Paired Promotion:** the exact Destination Sentinel in `../qmp-qemu-socket/docs/handoff.md` reports `HANDOFF VERIFIED`. Its state receipt binds both repository revisions, indices, diffs and complete porcelain state, all `52` retained xSAR compiler inputs and the qualified consumer. This permits preserved local `.trigger-tree/` files without treating an arbitrary dirty tree as verified.
3. Do not run baseline `cargo check`, `cargo test`, `clippy`, `rustdoc`, or `release` builds before starting new work-product when `PASSED (Cached)` holds. Preserve all unrelated changes, evidence, and the Git index.

---

## 1. Retained Baseline Receipt (`xsar 0.9.0`)

| Field | Value | Notes |
| :--- | :--- | :--- |
| **Package / Owner** | `xsar` | Core zero-dependency-default library crate |
| **Current Manifest Version** | `0.9.0` | Retained host-qualified baseline |
| **Provisional Target Version** | `None` | Next qualifying code revision earns `0.9.1` (patch) or `0.10.0` (agreed minor) after Tier 2 gate |
| **`Baseline_HEAD_SHA`** | `e50f7b3222fbd803d9eeecd8a2a37ba6fec856ca` | Qualified pre-promotion base; the paired Destination Sentinel records the exact published revision |
| **Porcelain State at Sign-Off** | `9 lines (Preserved Dirty Baseline)` | `9`–`11` lines when `docs/milestones.md` & `docs/handoff.md` are present |
| **Tier 2 Library Suite Status** | `PASSED (Cached)` | `296` `xsar` tests passed (`0` failed/ignored) + retained Miri qualification |
| **Active Feature Matrix** | `default-features = false` + `7` consumer features | `qmp`, `png`, `image_matching`, `pyramid_json`, `pyramid_search`, `execution`, `tcp_capture` |
| **Primary Consumer State** | `../qmp-qemu-socket` `2.9.153` | Qualified base `23016ba497f543a5aeee7bf97fe00e0a3c826b28`; `1,522` full tests, `782` sealed inputs and all `6` gates passed; fresh guest acceptance remains separate |

Charlie's signed-push request and separate approval to publish xSAR authorise the reviewed nine-file promotion, xSAR first. The consumer retains `path = "../xsar"`, dependency-free defaults and its seven existing features. Its milestone and handoff receipts record the resulting xSAR commit; a path remains a local dependency rather than a Git pin. The retained `296` tests and `3` Miri checks are cached evidence, with no new library code, version increment or full-suite run for this documentation update. Local `.trigger-tree/` files stay outside the commit. The pre-promotion check below remains a historical entry gate; after publication use the exact paired Destination Sentinel.

---

## 2. Read-Only Pre-Promotion Baseline Sentinel Check (zsh-Safe)

Run this check from the `xsar` repository root via `ctx_execute` at session start:

```zsh
# Verify xsar HEAD SHA, preserved porcelain count (9..11), Cargo.toml version, and consumer ../qmp-qemu-socket linkage
() {
  local xsar_head="$(git rev-parse HEAD 2>/dev/null)"
  local xsar_dirty="$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
  local xsar_ver="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
  local app_head="$(git -C ../qmp-qemu-socket rev-parse HEAD 2>/dev/null)"
  local app_ver="$(sed -n 's/^version = "\(.*\)"/\1/p' ../qmp-qemu-socket/Cargo.toml | head -n 1)"

  if [[ ! -f "docs/milestones.md" ]]; then
    printf "HANDOFF REGRESSION: Missing canonical tracker at docs/milestones.md\n"
    return 1
  fi
  if [[ "$xsar_head" != "e50f7b3222fbd803d9eeecd8a2a37ba6fec856ca" ]]; then
    printf "HANDOFF REGRESSION: xsar HEAD is %s (expected e50f7b3222fbd803d9eeecd8a2a37ba6fec856ca)\n" "$xsar_head"
    return 1
  fi
  if [[ "$xsar_ver" != "0.9.0" ]]; then
    printf "HANDOFF REGRESSION: xsar Cargo.toml version is %s (expected 0.9.0)\n" "$xsar_ver"
    return 1
  fi
  if (( xsar_dirty < 9 || xsar_dirty > 11 )); then
    printf "HANDOFF REGRESSION: xsar porcelain count is %s (expected 9..11 for preserved 0.9.0 baseline)\n" "$xsar_dirty"
    return 1
  fi
  if [[ "$app_head" != "23016ba497f543a5aeee7bf97fe00e0a3c826b28" ]]; then
    printf "HANDOFF REGRESSION: Consumer ../qmp-qemu-socket HEAD is %s (expected 23016ba497f543a5aeee7bf97fe00e0a3c826b28)\n" "$app_head"
    return 1
  fi

  printf "State: xsar=%s (dirty=%s, baseline 9..11), consumer qmp-qemu-socket=%s\n" "$xsar_ver" "$xsar_dirty" "$app_ver"
  printf "HANDOFF VERIFIED\n"
}

