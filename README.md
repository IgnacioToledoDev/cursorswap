# cambia-cursor

Switch the system cursor theme on Arch Linux / Hyprland by name or short alias, with an interactive TUI that previews each theme.

## What it changes

Applying a theme updates all of these, in this order (a failing step does not stop the others):

| Step | Effect |
|------|--------|
| `icon-path` | Symlinks the theme into `$XDG_DATA_HOME/icons` (default `~/.local/share/icons`) when it is not in a standard icon directory |
| `hypr-envs` | Rewrites `XCURSOR_THEME` and `HYPRCURSOR_THEME` in `~/.config/hypr/envs.conf` (other lines and the size are untouched) |
| `gtk-settings` | Sets `gtk-cursor-theme-name` in `~/.config/gtk-3.0/settings.ini` |
| `gsettings` | `gsettings set org.gnome.desktop.interface cursor-theme <name>` |
| `hyprctl` | `hyprctl setcursor <name> <size>` for an immediate change (size read from `XCURSOR_SIZE`, default 24) |

The name sent to the system is the theme **directory name**, not the name or alias you registered it under.
Config files that are symlinks (for example managed dotfiles) are written through the link, so the link is preserved.

## Usage

```sh
cambia-cursor                  # open the TUI
cambia-cursor Nordzy-cursors   # apply by name
cambia-cursor N                # apply by alias
cambia-cursor list             # registered themes, * marks the active one
cambia-cursor current          # active theme
cambia-cursor add ~/dotfiles/cursor/Nordzy-cursors --alias N [--name Nordic]
cambia-cursor remove N
```

`add` registers the directory in place (nothing is copied). The directory must contain `index.theme` and `cursors/`.
Aliases are 1-8 ASCII alphanumeric characters and cannot be one of the subcommand names.
A theme named like a subcommand can only be selected through its alias.

### TUI keys

| Key | Action |
|-----|--------|
| `j` / `k`, arrows | Move selection |
| `Enter` | Apply the selected theme (stays open, shows the per-step result) |
| `a` | Add a theme (path, then optional alias) |
| `d` | Remove the selected theme (`y` confirms) |
| `q`, `Esc`, `Ctrl-C` | Quit (`Esc` first cancels a prompt) |

The preview uses the terminal's graphics protocol when available (kitty, sixel, iTerm2) and falls back to half-block characters.

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (I/O, invalid input, registry error, or every step failed) |
| 2 | Theme not found |
| 3 | Partial apply (some steps failed, some succeeded) |

## Registry

`$XDG_CONFIG_HOME/cambia-cursor/registry.toml` (default `~/.config/cambia-cursor/registry.toml`):

```toml
[[cursor]]
name = "Nordzy-cursors"
alias = "N"
path = "/home/me/dotfiles/cursor/Nordzy-cursors"
```

## Architecture

Layered, with dependencies pointing inward: `presentation -> application -> domain <- infrastructure`.

```
src/
├── domain/          types, validation rules, ports (traits)
├── application/     use cases: list, add, apply, remove, current
├── infrastructure/  TOML registry, config-file and command appliers, Xcursor preview, atomic writes
├── presentation/    clap CLI, ratatui TUI
└── bootstrap.rs     composition root (the only place that knows concrete implementations)
```

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests never touch the real `$HOME`, the real cursor themes, `hyprctl` or `gsettings`; they use temporary directories and fakes.

## Known limitations

- Hyprland/GTK3/gsettings only; no GTK4-specific file and no native hyprcursor (`manifest.hl`) support.
- `list` and `current` read the active theme from `XCURSOR_THEME` in `envs.conf`.
- The path prompt in the TUI does not expand `~`.
- In the half-block fallback, transparent pixels render as black.
