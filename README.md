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
- **Find and install from the AUR.** `cursorswap search` and `cursorswap install` use `yay`, with a confirmation first and yay's own PKGBUILD review. The terminal UI can do the same without leaving it.
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
| `/` | Search the AUR: type a term, then `Enter` (see [Search and install from the UI](#search-and-install-from-the-ui)) |
| `d` | Remove the selected theme (`y` to confirm) |
| `q`, `Esc`, `Ctrl-C` | Quit (`Esc` first cancels a prompt or confirmation) |

The left pane lists your themes with the active one marked. The right pane previews the selected theme and animates it if it is an animated cursor. If a preview cannot be loaded, the UI shows `no preview` and keeps working.

The preview shows one cursor state at a time, named in the top-right of the preview pane. `[` and `]` cycle through the states: `default`, `pointer` (the link hand), `text`, `wait`, `progress`, `not-allowed`, `help`, `grabbing`, then `crosshair`, `move`, `nwse-resize` and `ew-resize`. `pointer` is the closest match for "hover over something clickable" and `grabbing` for "pressed". The chosen state stays selected when you move to another theme. If a theme has no cursor for a state, the pane says `no preview for <state>` and you can keep browsing. Each state is read the first time you select it.

#### Search and install from the UI

Press `/`, type a search term and press `Enter`. The left pane then lists the matching packages (the same cursor-theme filter as `cursorswap search`) with their name, version and description; packages you already have are marked `[installed]`.

| Key | Action |
|-----|--------|
| `j` / `k` or arrow keys | Move through the results |
| `Enter` | Ask to install the highlighted package (`y` to confirm, any other key cancels) |
| `/` | Start a new search |
| `Esc` | Close the results |

After you confirm, the UI steps aside: `yay` runs in your terminal, shows the PKGBUILD for review and asks for your sudo password, exactly like `cursorswap install`. When it finishes, press `Enter` to return. The new theme is registered, selected in the list and ready to preview. The terminal is restored even if `yay` fails.

Problems never close the UI. A missing `yay`, a search without results, an unreachable package index or a failed install show a message in the status line and leave you where you were.

Note: the path prompt does not expand `~`; type the full path, or use the command line `add`, where your shell expands it.

The release notes are generated by GitHub from the merged pull requests since the previous release and grouped by label as configured in `.github/release.yml`: `breaking-change` (Breaking changes), `enhancement` (New features), `bug` (Bug fixes), `documentation` (Documentation) and everything else under Other changes. Label a pull request before merging it to place it in a category, or give it `ignore-for-release` to leave it out of the notes.

Contributions are welcome. Please keep the layering, add tests with your change, and use conventional commit messages.

## License

MIT. See [LICENSE](LICENSE).
