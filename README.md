# cursorswap

Switch your cursor theme on Linux with one short command, and preview themes in a terminal UI before you commit to one.

```sh
cursorswap Nordzy-cursors   # apply by name
cursorswap N                # apply by alias
cursorswap                  # browse themes with a live preview
```

`cursorswap` keeps a small registry of the cursor themes you have already downloaded. Pick one by name or by a short alias of your choice, and it updates every place that stores the cursor setting: the Hyprland environment, GTK, gsettings, and the running session. No logout needed.

> **Status:** early release (MVP). Built for Arch Linux with Hyprland (including Omarchy). Other desktops are not supported yet; see [Limitations](#limitations).

## Features

- **Switch by name or alias.** `cursorswap Nordzy-cursors` or `cursorswap N`.
- **Terminal UI with preview.** Browse your themes and see the cursor, animated when the theme is animated.
- **Bring your own themes.** Register any cursor theme directory you already have; nothing is copied or moved.
- **Applies everywhere.** Hyprland env, GTK 3, gsettings and the live session, in one step.
- **Fail-safe.** If one step fails, the others still run and you get a per-step report and a distinct exit code.
- **Safe file edits.** Only the cursor lines are changed. Writes are atomic, and config files that are symlinks (for example, managed dotfiles) are written through the link so the link is preserved.
- **Scriptable.** Plain-text output and documented exit codes.

## Requirements

- Linux with Hyprland (`hyprctl` in your `PATH`)
- `gsettings` (from GLib) for the GTK/GNOME setting
- Rust toolchain to build from source (a recent stable release; the crate uses Rust edition 2024)
- For the best preview: a terminal that supports an image protocol (kitty, WezTerm, foot with sixel, iTerm2, ...). Other terminals fall back to Unicode half-block characters.

`hyprctl` and `gsettings` are only needed for their own step. If one is missing, that step is reported as failed and the rest still apply.

## Installation

From a clone of this repository:

```sh
cargo install --path .
```

This installs the `cursorswap` binary into `~/.cargo/bin`. Make sure that directory is in your `PATH`.

Or build without installing:

```sh
cargo build --release
./target/release/cursorswap --help
```

If you like a shorter command, add an alias to your shell config:

```sh
alias cc=cursorswap
```

### Uninstall

```sh
cargo uninstall cursorswap
rm -r ~/.config/cursorswap       # optional: removes your registry
```

Symlinks that `cursorswap` created in `~/.local/share/icons` are never deleted automatically; remove them by hand if you want to.

## Quick start

1. **Register the cursor themes you already have.** A theme directory must contain `index.theme` and a `cursors/` folder.

   ```sh
   cursorswap add ~/Downloads/Nordzy-cursors --alias N
   cursorswap add /usr/share/icons/Bibata-Modern-Classic --alias B
   ```

2. **See what is registered.** The active theme is marked with `*`.

   ```sh
   cursorswap list
   ```

3. **Apply one.**

   ```sh
   cursorswap B
   ```

4. **Or browse visually.**

   ```sh
   cursorswap
   ```

## Usage

```
cursorswap [QUERY]
cursorswap <COMMAND>
```

| Command | Description |
|---------|-------------|
| `cursorswap` | Open the terminal UI |
| `cursorswap <QUERY>` | Apply the theme whose name or alias matches `QUERY` |
| `cursorswap list` | List registered themes; `*` marks the active one |
| `cursorswap current` | Show the active theme |
| `cursorswap add <PATH> [--name <NAME>] [--alias <ALIAS>]` | Register a theme directory |
| `cursorswap remove <QUERY>` | Remove a theme from the registry (files are not touched) |
| `cursorswap --help` / `--version` | Help and version |

### Names and aliases

- A query is matched against theme **names** first (case-insensitive), then **aliases**.
- By default a theme's name is its directory name. Override it with `--name`.
- An alias is 1 to 8 ASCII letters or digits, for example `N` or `nord`. It cannot be the same as another theme's name or alias.
- `list`, `add`, `remove` and `current` are subcommands. A theme named like one of them can only be applied through its alias.

### Registering and removing themes

`add` records the directory where it is. Nothing is copied, so you can point at `/usr/share/icons`, a folder in your dotfiles, or anywhere else. If the theme lives outside a standard icon directory, applying it creates a symlink in `~/.local/share/icons` so that GTK and Hyprland can find it.

`remove` only deletes the registry entry. It never deletes theme files or symlinks.

### Terminal UI

Run `cursorswap` with no arguments.

| Key | Action |
|-----|--------|
| `j` / `k` or arrow keys | Move the selection |
| `Enter` | Apply the selected theme (the UI stays open and shows the result of each step) |
| `a` | Add a theme: type its path, then an optional alias |
| `d` | Remove the selected theme (`y` to confirm) |
| `q`, `Esc`, `Ctrl-C` | Quit (`Esc` first cancels a prompt or confirmation) |

The left pane lists your themes with the active one marked. The right pane previews the selected theme and animates it if it is an animated cursor. If a preview cannot be loaded, the UI shows `no preview` and keeps working.

Note: the path prompt does not expand `~`; type the full path, or use the command line `add`, where your shell expands it.

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (I/O, invalid input, registry problem, or every step failed) |
| `2` | Theme not found |
| `3` | Partial apply: some steps succeeded and some failed |

Example in a script:

```sh
cursorswap Nordzy-cursors
case $? in
  0) echo "cursor changed" ;;
  2) echo "unknown theme" ;;
  3) echo "applied with problems, check the output" ;;
  *) echo "failed" ;;
esac
```

## What gets changed

Applying a theme runs these steps in order. Each step reports `ok <step>` or `failed <step>: <reason>`.

| Step | What it does |
|------|--------------|
| `icon-path` | If the theme is not in `/usr/share/icons`, `~/.local/share/icons` or `~/.icons`, creates a symlink to it in `$XDG_DATA_HOME/icons` (default `~/.local/share/icons`). Existing files and links are never overwritten or deleted. |
| `hypr-envs` | Sets `XCURSOR_THEME` and `HYPRCURSOR_THEME` in `~/.config/hypr/envs.conf`. All other lines, including `XCURSOR_SIZE`, are left as they are. A missing line is appended. The file must already exist. |
| `gtk-settings` | Sets `gtk-cursor-theme-name` in the `[Settings]` section of `~/.config/gtk-3.0/settings.ini` (created if missing). |
| `gsettings` | Runs `gsettings set org.gnome.desktop.interface cursor-theme <name>`. |
| `hyprctl` | Runs `hyprctl setcursor <name> <size>` to change the cursor immediately. The size comes from `XCURSOR_SIZE` in `envs.conf` (default 24). |

Good to know:

- The theme name written to these places is the **directory name** of the theme, which is what the system looks up. It can differ from the name or alias you registered it under.
- Theme names containing control characters, commas (in `envs.conf`) or a leading `-` are rejected, so a name can never inject extra lines into a config file or be read as a command option.
- There is no dry-run mode. If you want a safety net, keep a copy of `~/.config/hypr/envs.conf` and `~/.config/gtk-3.0/settings.ini` before your first run.
- To go back, apply your previous theme again (`cursorswap <previous>`).

## Configuration

The registry is a plain TOML file at `$XDG_CONFIG_HOME/cursorswap/registry.toml` (default `~/.config/cursorswap/registry.toml`). You can edit it by hand:

```toml
[[cursor]]
name = "Nordzy-cursors"
alias = "N"
path = "/home/me/dotfiles/cursor/Nordzy-cursors"

[[cursor]]
name = "Bibata-Modern-Classic"
path = "/usr/share/icons/Bibata-Modern-Classic"
```

`alias` is optional. An invalid entry makes the tool stop with an error naming the entry; it never overwrites a registry it cannot parse.

Locations are derived from these environment variables:

| Variable | Used for | Default |
|----------|----------|---------|
| `XDG_CONFIG_HOME` | Registry, `hypr/envs.conf`, `gtk-3.0/settings.ini` | `$HOME/.config` |
| `XDG_DATA_HOME` | Where theme symlinks are created (`icons/`) | `$HOME/.local/share` |
| `HOME` | Fallback for both | none |

## Troubleshooting

**`error: no cursor matches "..."` (exit 2).** The query is neither a registered name nor an alias. Run `cursorswap list`.

**Exit code 3 / a `failed` line.** Read the reason after `failed <step>:`. Common causes: `hyprctl` is not in `PATH` or Hyprland is not running, `gsettings` is not installed, or `~/.config/hypr/envs.conf` does not exist.

**The cursor changed but some apps still show the old one.** Already-running applications may cache the cursor. Restart the app, or log out and back in.

**The preview looks blocky.** Your terminal does not support an image protocol, so half-block characters are used. Try kitty, WezTerm or another terminal with graphics support. In this fallback, transparent pixels render as black.

**`add` says the path is not a valid theme.** The directory needs an `index.theme` file and a `cursors/` folder inside it.

**An alias is rejected.** Aliases are 1 to 8 ASCII letters or digits, must be unique, and cannot be `list`, `add`, `remove` or `current`.

## Limitations

- Hyprland, GTK 3 and gsettings only. There is no GTK 4 specific file, and no native hyprcursor (`manifest.hl`) support.
- `list` and `current` read the active theme from `XCURSOR_THEME` in `envs.conf`, not from gsettings or the running session.
- Downloading themes is out of scope; bring your own.
- Cursor size is read, never changed.
- Only the Arch Linux + Hyprland setup has been targeted.

## Development

The code is layered, with dependencies pointing inward: `presentation -> application -> domain <- infrastructure`.

```
src/
├── domain/          types, validation rules and ports (traits)
├── application/     use cases: list, add, apply, remove, current
├── infrastructure/  TOML registry, config-file and command appliers, Xcursor preview, atomic writes
├── presentation/    clap CLI and ratatui TUI
└── bootstrap.rs     composition root, the only place that knows concrete implementations
```

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests never touch your real home directory, your real cursor themes, `hyprctl` or `gsettings`: they use temporary directories and fakes. The interactive TUI needs a real terminal, so its event loop is not covered by automated tests.

Contributions are welcome. Please keep the layering, add tests with your change, and use conventional commit messages.
