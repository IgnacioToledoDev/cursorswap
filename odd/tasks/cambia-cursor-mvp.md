# Feature: cambia-cursor-mvp

## Objective
Rust CLI + ratatui TUI to switch the system cursor theme on Arch/Hyprland (Omarchy) by name or short alias, with a registry of already-downloaded themes and a preview.

## Problem / why
Cursor theme lives in 4 places (`~/.config/hypr/envs.conf`, `~/.config/gtk-3.0/settings.ini`, gsettings, live `hyprctl`). Switching by hand is tedious; there is no catalog of the 6+ themes in `~/workspace/dotfiles/arch/cursor` and 16 in `/usr/share/icons`.

## Scope (approved by user)
- Apply to all 4 sites: `hyprctl setcursor`, `envs.conf`, `settings.ini`, gsettings. Keep current `XCURSOR_SIZE`.
- `add` registers by path (no copy) in `~/.config/cambia-cursor/registry.toml`; symlink into `~/.local/share/icons/` on apply if theme is not in a standard search path.
- CLI positional: `cambia-cursor <name|alias>`; no args opens TUI; subcommands `list`, `add`, `remove`, `current`.
- Preview: crates `xcursor` + `ratatui-image` (protocol auto-detect, half-block fallback).
- Layers: domain / application / infrastructure / presentation. Deps: presentation -> application -> domain <- infrastructure. `main.rs` is the only composition root.
- Out of scope: downloading themes, native hyprcursor (`manifest.hl`), other compositors/distros, size change in TUI.

## Constraints
- Alias: optional, 1-8 alphanumeric, unique, cannot equal another cursor name or a subcommand. Name/alias lookup case-insensitive (exact name first, then alias).
- File rewrites atomic (temp + rename); only cursor lines change.
- One applier failing must not block the others; report each; distinct exit codes (not found / partial / error).
- Errors: `thiserror` per layer, `anyhow` only in `main`.
- Artifacts (code, comments, docs, commits) in English; conventional commits, no AI attribution.
- Tests must never touch the real `$HOME` (tempfile dirs).

## Execution config
- TDD: enabled (source: user global config "Strict TDD Mode"); runner: `cargo test`. RED observed before implementation, then GREEN, then REFACTOR.
- Branch: `feat/cambia-cursor-mvp` in `~/workspace/cambia-cursor`.
- RDD: not started here; assess each work-unit commit per ODD protocol when enabled.
- Delivery: forecast ~1500 authored changed lines (> 400). Strategy: ask-on-risk; chain strategy chosen by user: `feature-branch-chain`.
- Slices (child branches cut from `feat/cambia-cursor-mvp`, which integrates to `main` at the end):
  - S1 `feat/cambia-cursor-domain-app`: T1, T2
  - S2 `feat/cambia-cursor-infra`: T3, T4, T5
  - S3 `feat/cambia-cursor-presentation`: T6, T7, T8
- Each slice merges into `feat/cambia-cursor-mvp` in order; no push/PR without user decision.

## Tasks
Route legend: D = delegated writer, I = inline. Trigger evidence: writer trigger (2+ non-trivial files).

- [x] T1 (D) Scaffold crate + domain: `CursorTheme`, `Alias` validation, ports (`CursorRepository`, `CursorApplier`, `PreviewLoader`), domain errors. Tests: alias/name rules.
  - Commit `c569ce9` on `feat/cambia-cursor-domain-app`. RED 0/8 then 8/22 passed, GREEN 22/22 (writer report). Parent spot check: `cargo test` 22 passed. Writer also ran clippy `-D warnings` and `fmt --check`, clean.
  - Native review: assessed high (`process_boundary` signal on `src/main.rs`, 449 lines), consent granted by user, 4 lenses approved, acknowledged, authority burned. Reviewed boundary is now `c569ce9`.
  - Non-blocking advisory findings, follow-ups only (not reopened): R2 name reported as alias in an error (`cursor.rs:78-83`, WARNING), R3-001 theme path unvalidated/ordering concern (`cursor.rs:42-52`, WARNING), plus 5 SUGGESTIONs (alias length magic constant, crate-wide `dead_code` allow in `main.rs`, unchecked constructor naming, R3-002, R3-003). Decide per task whether to fold into T2/T6.
- [x] T2 (D) Application use cases `list/add/apply/remove/current` with in-memory fakes. Tests: resolve order, uniqueness, partial apply reporting.
  - Commit `a2f253d` on `feat/cambia-cursor-domain-app`. Writer RED: 30/52 failed on behavior (stubs), GREEN 52/52. Parent spot check: `cargo test` 52 passed. Writer: clippy `-D warnings` and `fmt --check` clean. New ports `ThemeInspector`, `CurrentCursorProvider` added in domain (no domain behavior change).
  - Native review: assessed high (same `src/main.rs` process signal, 663 lines vs boundary `c569ce9`), user granted again, 4 lenses approved, acknowledged, authority burned. Reviewed boundary now `a2f253d`.
  - Advisory (non-blocking, not reopened): WARNINGs `apply.rs:32-44` (R3-001) and `apply.rs:38-39` (R4-001) on the applier-loop path, `add.rs:22-25` (R2-001); SUGGESTIONs R2-002, R2-003, R2-004, R3-002, R4-002. Revisit `apply.rs` ones when wiring T6.
  - Slice S1 (T1, T2) complete on `feat/cambia-cursor-domain-app`. No merge/push done (user decision). S2 is cut from the S1 tip so it can use this code.
- [x] T3 (D) Infra `registry_toml` repository + `FsThemeInspector`. Tests: roundtrip, missing file, duplicate, tempdir.
  - Commit `049f509` on `feat/cambia-cursor-infra`. Writer RED 0/9 (inspector) and 0/16 (registry) with stubs, GREEN 9/9 and 16/16. Parent spot check: `cargo test` 77 passed. Writer: clippy `-D warnings` and `fmt --check` clean. Deps added: serde, toml, tempfile (dev).
  - Native review: assessed high (same `src/main.rs` signal, 684 lines vs boundary `a2f253d`), user granted a third time, 4 lenses approved, acknowledged, authority burned. Reviewed boundary now `049f509`.
  - Advisory (non-blocking, not reopened), all in `registry_toml.rs`: WARNINGs R2-001 (124-141), R3-001 (126), R4-001 (90-98, atomic temp write), R4-002 (98, rename step); SUGGESTIONs R3-002, R3-003, R3-004, R4-003. Candidate hardening for T4 since appliers reuse the same atomic-write pattern.
  - Cleanup owed in T6: remove `#[allow(unused_imports)]` in `infrastructure/mod.rs` and the crate-wide `dead_code` allow once wired.
- [ ] T4 (D) Infra appliers `hyprctl`, `hypr_envs`, `gtk_settings`, `gsettings` + icon-path symlink. Tests: tempdir rewrite preserves other lines; command runner faked.
- [ ] T5 (D) Infra `xcursor_preview` (frames of `left_ptr`, animation). Tests: small Xcursor fixture.
- [ ] T6 (D) Presentation CLI (clap) + `main.rs` composition root + exit codes. Tests: clap parsing, exit-code mapping.
- [ ] T7 (D) Presentation TUI (list + preview, keys j/k/Enter/a/d/q). Tests: `TestBackend` render, key handling.
- [ ] T8 (I) README, final `cargo test` + `cargo clippy` + manual smoke of `list`/`current` (no real apply without user ok).

## Acceptance criteria
- `cambia-cursor Nordzy-cursors` and `cambia-cursor N` both resolve and apply to the 4 sites.
- `add` rejects paths without `index.theme` + `cursors/`.
- TUI shows list with active marker and a preview; no-preview fallback works.
- `cargo test` green; `cargo clippy -- -D warnings` clean.

## Progress / evidence
- Design approved by user in brainstorming (4 questions answered). Spec doc not written separately: this document is the single source.
- Branch and repo created, no source written yet.
- Engram mirror `odd/cambia-cursor-mvp/tasks`: PENDING. `mem_save` refused (`session_project_mismatch`: session bound to project "nocti", repo is "cambia-cursor"). Resync once a session bound to this repo exists.

## Next step
Resolve delivery chain strategy, then start T1 (RED first).
