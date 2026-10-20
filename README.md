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
- **Find and install from the AUR.** `cursorswap search` and `cursorswap install` use `yay`, with a confirmation first and yay's own PKGBUILD review.
- **Applies everywhere.** Hyprland env, GTK 3, gsettings and the live session, in one step.
- **Fail-safe.** If one step fails, the others still run and you get a per-step report and a distinct exit code.
- **Safe file edits.** Only the cursor lines are changed. Writes are atomic, and config files that are symlinks (for example, managed dotfiles) are written through the link so the link is preserved.
- **Scriptable.** Plain-text output and documented exit codes.

## Requirements

- Linux with Hyprland (`hyprctl` in your `PATH`)
- `gsettings` (from GLib) for the GTK/GNOME setting
- Rust toolchain to build from source (a recent stable release; the crate uses Rust edition 2024)
- For the best preview: a terminal that supports an image protocol (kitty, WezTerm, foot with sixel, iTerm2, ...). Other terminals fall back to Unicode half-block characters.

- `yay` (and `pacman`), only for `search` and `install`; see [Installing themes from the AUR](#installing-themes-from-the-aur)

`hyprctl` and `gsettings` are only needed for their own step. If one is missing, that step is reported as failed and the rest still apply.

## Installation

There are no prebuilt releases yet, so the installer builds from source with Cargo. You need a Rust toolchain ([rustup.rs](https://rustup.rs)) and `git`.

```sh
curl -fsSL https://raw.githubusercontent.com/IgnacioToledoDev/cursorswap/main/install.sh | sh
```

This installs `cursorswap` into `~/.local/bin`. Running it again upgrades to the latest `main`. The script never uses `sudo` and writes only inside the install prefix. If `~/.local/bin` is not in your `PATH`, it prints the line to add.

Piping a script into a shell runs whatever the server sends. If you prefer to check it first, download, read, then run:

```sh
curl -fsSLO https://raw.githubusercontent.com/IgnacioToledoDev/cursorswap/main/install.sh
less install.sh
sh install.sh
```

To pass options through the one-liner, use `sh -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/IgnacioToledoDev/cursorswap/main/install.sh | sh -s -- --prefix "$HOME/.cargo"
```

| Option | Description |
|--------|-------------|
| `--prefix DIR` | Install to `DIR/bin/cursorswap` (default: `~/.local`) |
| `--from-path DIR` | Install from a local checkout instead of GitHub |
| `--ref REF` | Install a branch or tag from GitHub (`v1.2.3` style names are tags) |
| `-h`, `--help` | Show help |

### Manual build

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

## Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/IgnacioToledoDev/cursorswap/main/uninstall.sh | sh
```

Or download and read it first, as above (`uninstall.sh`). Options:

| Option | Description |
|--------|-------------|
| `--prefix DIR` | Prefix the binary was installed into (default: `~/.local`) |
| `--purge-config` | Also remove `~/.config/cursorswap` (your registry) without asking |
| `--keep-config` | Keep the config without asking |
| `-h`, `--help` | Show help |

What it does:

- Removes the `cursorswap` binary from the prefix (through `cargo uninstall` when available) and nothing else in it.
- Asks before removing `~/.config/cursorswap`, defaulting to No. When it has no terminal (for example when piped from `curl`), it keeps the config and tells you; pass `--purge-config` to remove it.
- Running it again is safe.

What it does not do: it never deletes theme directories or the symlinks `cursorswap` created in `~/.local/share/icons`. It lists the symlinks that point to themes in your registry, with the `rm` command for each, so you can remove them by hand. If you installed with plain `cargo install` into `~/.cargo`, use `cargo uninstall cursorswap`, or pass `--prefix "$HOME/.cargo"`.

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
| `cursorswap search <TERM>` | Search the AUR (through `yay`) for cursor themes |
| `cursorswap install <PACKAGE> [--alias <ALIAS>]` | Install an AUR package with `yay` and register the cursor themes it provides |
| `cursorswap update [--check] [--yes]` | Check for a newer release and install it (see [Updating](#updating)) |
| `cursorswap --help` / `--version` | Help and version |

### Names and aliases

- A query is matched against theme **names** first (case-insensitive), then **aliases**.
- By default a theme's name is its directory name. Override it with `--name`.
- An alias is 1 to 8 ASCII letters or digits, for example `N` or `nord`. It cannot be the same as another theme's name or alias.
- `list`, `add`, `remove`, `current`, `search`, `install` and `update` are subcommands. A theme named like one of them can only be applied through its alias.

### Registering and removing themes

`add` records the directory where it is. Nothing is copied, so you can point at `/usr/share/icons`, a folder in your dotfiles, or anywhere else. If the theme lives outside a standard icon directory, applying it creates a symlink in `~/.local/share/icons` so that GTK and Hyprland can find it.

`remove` only deletes the registry entry. It never deletes theme files or symlinks.

### Installing themes from the AUR

`search` and `install` need [`yay`](https://github.com/Jguer/yay) in your `PATH`. Without it they stop with an error that points to the yay install instructions.

```sh
cursorswap search nordzy                    # list cursor themes matching "nordzy"
cursorswap install nordzy-cursors           # install and register
cursorswap install nordzy-cursors --alias N # same, with an alias
```

`search` runs `yay -Ss <TERM>` and prints the packages whose name or description contains "cursor" (case-insensitive), with their version and description. Packages that are already installed are marked `[installed]`. The filter is a heuristic: it can show non-theme packages, and it can miss themes that never use the word "cursor".

`install` works in these steps:

1. It checks that `yay` is available and that the package name is valid.
2. It shows the exact package name and asks for confirmation. The default answer is **no**. If standard input is not a terminal, it stops with an error instead of assuming yes.
3. It runs `yay -S <package>` attached to your terminal. yay itself shows the PKGBUILD for review and asks for your sudo password.
4. It lists the files of the installed package (`pacman -Ql`) and registers every directory that contains `index.theme` and `cursors/`, named after the directory.
5. Themes that are already registered (same path or same name) are skipped and reported, not treated as an error.

Safety behavior:

- `cursorswap` never passes `--noconfirm` to yay and never runs `sudo` itself. Reviewing the PKGBUILD and entering your password stay your decisions, inside yay.
- Nothing is installed unless you answer `y` or `yes`.
- A package that provides no valid cursor theme registers nothing and exits with code 1 (the package itself stays installed; remove it with your package manager).
- `--alias` only works when the package provides exactly one theme. The number of themes is only known after the install, so with several themes the command exits with code 1, registers nothing and lists the directories; register them with `cursorswap add <PATH> --alias <ALIAS>`. An invalid or reserved alias is rejected before anything is installed.
- If `yay -S` fails or you abort it, nothing is registered.

### Terminal UI

Run `cursorswap` with no arguments.

| Key | Action |
|-----|--------|
| `j` / `k` or arrow keys | Move the selection |
| `[` / `]` | Show the previous / next cursor state in the preview |
| `Enter` | Apply the selected theme (the UI stays open and shows the result of each step) |
| `a` | Add a theme: type its path, then an optional alias |
| `d` | Remove the selected theme (`y` to confirm) |
| `q`, `Esc`, `Ctrl-C` | Quit (`Esc` first cancels a prompt or confirmation) |

The left pane lists your themes with the active one marked. The right pane previews the selected theme and animates it if it is an animated cursor. If a preview cannot be loaded, the UI shows `no preview` and keeps working.

The preview shows one cursor state at a time, named in the top-right of the preview pane. `[` and `]` cycle through the states: `default`, `pointer` (the link hand), `text`, `wait`, `progress`, `not-allowed`, `help`, `grabbing`, then `crosshair`, `move`, `nwse-resize` and `ew-resize`. `pointer` is the closest match for "hover over something clickable" and `grabbing` for "pressed". The chosen state stays selected when you move to another theme. If a theme has no cursor for a state, the pane says `no preview for <state>` and you can keep browsing. Each state is read the first time you select it.

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

The registry is a plain TOML file at `$XDG_CONFIG_HOME/cursorswap/conf/registry.toml` (default `~/.config/cursorswap/conf/registry.toml`). You can edit it by hand:

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

## Updating

When you run `cursorswap` from a terminal, it checks GitHub for a newer **stable** release (2 second timeout, result cached for 6 hours in `~/.config/cursorswap/conf/update-state.toml`). If one exists you are asked:

```
A new version of cursorswap is available: 0.2.0 (you have 0.1.0).
Update now? [y]es / [n]o / [s]kip this version
```

- **yes** downloads the release, verifies its SHA-256 checksum, replaces the binary atomically and re-runs your command on the new version. The TUI shows the same choice as a modal.
- **no** continues; you are asked again once the cache expires.
- **skip** remembers that version and stays quiet until a newer one is released.

Any failure (offline, rate limited, bad response) is silent and never blocks your command. Nothing is installed without an explicit yes.

```sh
cursorswap update --check   # print installed and latest versions; exit 0 up to date, 10 update available, 1 error
cursorswap update           # install the latest release after confirmation
cursorswap update --yes     # install without asking
```

The check never runs, and nothing is downloaded, when stdin or stdout is not a terminal, when `CI` is set, or when you opt out with `--no-update-check`, `CURSORSWAP_NO_UPDATE_CHECK=1`, or `update_check = false` in `~/.config/cursorswap/conf/config.toml`. Scripts keep their current behavior and exit codes.

A binary managed by a package manager (under `/usr/`) or installed with `cargo install` is never overwritten in place: you get a notice with the command to update it instead. Release binaries are published for `x86_64-unknown-linux-gnu`.

## Troubleshooting

**`error: no cursor matches "..."` (exit 2).** The query is neither a registered name nor an alias. Run `cursorswap list`.

**Exit code 3 / a `failed` line.** Read the reason after `failed <step>:`. Common causes: `hyprctl` is not in `PATH` or Hyprland is not running, `gsettings` is not installed, or `~/.config/hypr/envs.conf` does not exist.

**The cursor changed but some apps still show the old one.** Already-running applications may cache the cursor. Restart the app, or log out and back in.

**The preview looks blocky.** Your terminal does not support an image protocol, so half-block characters are used. Try kitty, WezTerm or another terminal with graphics support. In this fallback, transparent pixels render as black.

**`install` says `yay` was not found.** Install yay first: https://github.com/Jguer/yay#installation

**`install` says standard input is not a terminal.** The confirmation needs an interactive terminal; run the command directly in one.

**`add` says the path is not a valid theme.** The directory needs an `index.theme` file and a `cursors/` folder inside it.

**An alias is rejected.** Aliases are 1 to 8 ASCII letters or digits, must be unique, and cannot be `list`, `add`, `remove`, `current`, `search`, `install` or `update`.

## Limitations

- Hyprland, GTK 3 and gsettings only. There is no GTK 4 specific file, and no native hyprcursor (`manifest.hl`) support.
- `list` and `current` read the active theme from `XCURSOR_THEME` in `envs.conf`, not from gsettings or the running session.
- Downloading themes is limited to AUR packages through `yay`; there is no other download source, and no AUR support without `yay`.
- Cursor size is read, never changed.
- Only the Arch Linux + Hyprland setup has been targeted.

## Development

The code is layered, with dependencies pointing inward: `presentation -> application -> domain <- infrastructure`.

```
src/
├── domain/          types, validation rules and ports (traits)
├── application/     use cases: list, add, apply, remove, current, search, install
├── infrastructure/  TOML registry, config-file and command appliers, yay/pacman adapter, terminal prompt, Xcursor preview, atomic writes
├── presentation/    clap CLI and ratatui TUI
└── bootstrap.rs     composition root, the only place that knows concrete implementations
```

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests never touch your real home directory, your real cursor themes, `hyprctl`, `gsettings`, `yay` or `pacman`: they use temporary directories and fakes. The interactive TUI needs a real terminal, so its event loop is not covered by automated tests.

### Releasing

Versions follow [Semantic Versioning 2.0.0](https://semver.org). Release tags are `v` plus the `Cargo.toml` version, without build metadata (`+...`). A tag with a hyphen, such as `v0.3.0-rc.1`, is a pre-release and never becomes GitHub's "Latest".

1. Run the "Prepare release" workflow with the new version (for example `0.2.0`), or bump `Cargo.toml` and `Cargo.lock` by hand in a pull request.
2. Merge that pull request into `main`.
3. From `main`, tag and push:

   ```sh
   git tag -a vX.Y.Z -m vX.Y.Z && git push origin vX.Y.Z
   ```

4. The Release workflow validates the tag, runs the tests, builds the binary and publishes it with its checksum and generated release notes. A stable tag must point to a commit already on `main`; pre-release tags may come from any branch.

The release notes are generated by GitHub from the merged pull requests since the previous release and grouped by label as configured in `.github/release.yml`: `breaking-change` (Breaking changes), `enhancement` (New features), `bug` (Bug fixes), `documentation` (Documentation) and everything else under Other changes. Label a pull request before merging it to place it in a category, or give it `ignore-for-release` to leave it out of the notes.

Contributions are welcome. Please keep the layering, add tests with your change, and use conventional commit messages.
