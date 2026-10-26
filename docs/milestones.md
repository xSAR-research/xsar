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

## 1. Active Baseline Receipt (`xsar 0.9.0`)

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
