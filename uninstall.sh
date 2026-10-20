#!/bin/sh
# Uninstall cursorswap. Never deletes theme directories or theme symlinks.
set -eu

usage() {
  cat <<'USAGE'
Usage: uninstall.sh [OPTIONS]

Remove the cursorswap binary installed by install.sh.

Options:
  --prefix DIR      Prefix the binary was installed into (default: $HOME/.local)
  --purge-config    Also remove $XDG_CONFIG_HOME/cursorswap (your registry), without asking
  --keep-config     Keep the config directory, without asking
  -h, --help        Show this help

Without --purge-config or --keep-config the script asks before removing the config,
defaulting to No. Without a terminal it keeps the config and says so.

Theme directories and the symlinks cursorswap created in $XDG_DATA_HOME/icons are never
removed; the script lists the symlinks and shows how to remove them by hand.
USAGE
}

err() { printf 'uninstall.sh: error: %s\n' "$*" >&2; }
die() { err "$*"; exit 1; }
usage_error() { err "$*"; usage >&2; exit 2; }

sh_quote() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

prefix=""
prefix_set=0
config_mode=ask

while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage; exit 0 ;;
    --prefix)
      [ $# -ge 2 ] || usage_error "--prefix requires a value"
      prefix=$2; prefix_set=1; shift 2 ;;
    --prefix=*) prefix=${1#--prefix=}; prefix_set=1; shift ;;
    --purge-config) config_mode=purge; shift ;;
    --keep-config) config_mode=keep; shift ;;
    *) usage_error "unknown option: $1" ;;
  esac
done

if [ "$prefix_set" -eq 0 ]; then
  [ -n "${HOME:-}" ] || die "HOME is not set; pass --prefix DIR"
  prefix="$HOME/.local"
fi

while [ "${prefix%/}" != "$prefix" ]; do prefix=${prefix%/}; done
[ -n "$prefix" ] || die "invalid prefix: it must be a non-empty directory other than /"
case $prefix in
  /*) ;;
  *) prefix="$PWD/$prefix" ;;
esac

config_base=${XDG_CONFIG_HOME:-}
[ -n "$config_base" ] || config_base=${HOME:+$HOME/.config}
data_base=${XDG_DATA_HOME:-}
[ -n "$data_base" ] || data_base=${HOME:+$HOME/.local/share}

bin="$prefix/bin/cursorswap"

# Binary
installed=0
if [ -e "$bin" ] || [ -L "$bin" ]; then
  installed=1
elif [ -f "$prefix/.crates.toml" ] && grep -q '^"cursorswap ' "$prefix/.crates.toml"; then
  installed=1
fi

if [ "$installed" -eq 0 ]; then
  printf 'cursorswap is not installed under %s; nothing to remove there.\n' "$prefix"
else
  if command -v cargo >/dev/null 2>&1 && [ -f "$prefix/.crates.toml" ]; then
    cargo uninstall --root "$prefix" cursorswap >/dev/null 2>&1 || true
  fi
  # Fallback when cargo is missing or left the file behind: remove only our binary.
  if [ -e "$bin" ] || [ -L "$bin" ]; then
    rm -f -- "$bin"
  fi
  if [ -e "$bin" ] || [ -L "$bin" ]; then
    die "could not remove $bin"
  fi
  printf 'Removed %s\n' "$bin"
fi

# Theme symlinks: report only
if [ -n "$config_base" ] && [ -n "$data_base" ]; then
  registry="$config_base/cursorswap/conf/registry.toml"
  icons="$data_base/icons"
  if [ -f "$registry" ] && [ -d "$icons" ]; then
    paths=$(sed -n 's/^[[:space:]]*path[[:space:]]*=[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$registry")
    found=""
    if [ -n "$paths" ]; then
      for link in "$icons"/* "$icons"/.[!.]*; do
        [ -L "$link" ] || continue
        target=$(readlink "$link") || continue
        while IFS= read -r p; do
          [ -n "$p" ] || continue
          case $target in
            "$p"|"$p"/*)
              found="$found  rm -- $(sh_quote "$link")
"
              break ;;
          esac
        done <<PATHS
$paths
PATHS
      done
    fi
    if [ -n "$found" ]; then
      printf '\nThese symlinks in %s point to themes in your registry.\n' "$icons"
      printf 'They were not removed. To remove one by hand:\n%s' "$found"
    fi
  fi
fi

# Config
config_dir=""
[ -n "$config_base" ] && config_dir="$config_base/cursorswap"

remove_config() {
  case $config_dir in
    ""|/|*/) die "refusing to remove config path '$config_dir'" ;;
    */cursorswap) ;;
    *) die "refusing to remove config path '$config_dir'" ;;
  esac
  rm -rf -- "$config_dir"
  printf 'Removed %s\n' "$config_dir"
}

if [ -z "$config_dir" ] || [ ! -e "$config_dir" ]; then
  exit 0
fi

case $config_mode in
  purge) remove_config ;;
  keep) printf 'Kept %s\n' "$config_dir" ;;
  ask)
    if [ -t 0 ]; then
      printf 'Remove %s (your cursor registry)? [y/N] ' "$config_dir"
      answer=""
      read -r answer || answer=""
      case $answer in
        y|Y|yes|YES|Yes) remove_config ;;
        *) printf 'Kept %s\n' "$config_dir" ;;
      esac
    else
      printf 'Kept %s (no terminal to ask). Re-run with --purge-config to remove it.\n' "$config_dir"
    fi
    ;;
esac
