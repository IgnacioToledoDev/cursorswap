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

- [ ] T1 (D) Scaffold crate + domain: `CursorTheme`, `Alias` validation, ports (`CursorRepository`, `CursorApplier`, `PreviewLoader`), domain errors. Tests: alias/name rules.
- [ ] T2 (D) Application use cases `list/add/apply/remove/current` with in-memory fakes. Tests: resolve order, uniqueness, partial apply reporting.
- [ ] T3 (D) Infra `registry_toml` repository. Tests: roundtrip, missing file, duplicate, tempdir.
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
