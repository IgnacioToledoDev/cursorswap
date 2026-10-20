#!/bin/sh
# Install cursorswap with cargo into a prefix. No sudo; nothing is written outside the prefix.
set -eu

REPO_URL="https://github.com/IgnacioToledoDev/cursorswap"

usage() {
  cat <<'USAGE'
Usage: install.sh [OPTIONS]

Build and install cursorswap with cargo. Re-running upgrades or reinstalls it.

Options:
  --prefix DIR      Install into DIR/bin (default: $HOME/.local)
  --from-path DIR   Install from a local checkout instead of GitHub
  --ref REF         Branch or tag to install from GitHub (a name like v1.2.3
                    is treated as a tag, anything else as a branch)
  -h, --help        Show this help

Requires: cargo (https://rustup.rs) and, for GitHub installs, git.
USAGE
}

err() { printf 'install.sh: error: %s\n' "$*" >&2; }
die() { err "$*"; exit 1; }
usage_error() { err "$*"; usage >&2; exit 2; }

prefix=""
prefix_set=0
from_path=""
ref=""

while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage; exit 0 ;;
    --prefix)
      [ $# -ge 2 ] || usage_error "--prefix requires a value"
      prefix=$2; prefix_set=1; shift 2 ;;
    --prefix=*) prefix=${1#--prefix=}; prefix_set=1; shift ;;
    --from-path)
      [ $# -ge 2 ] || usage_error "--from-path requires a value"
      from_path=$2; shift 2 ;;
    --from-path=*) from_path=${1#--from-path=}; shift ;;
    --ref)
      [ $# -ge 2 ] || usage_error "--ref requires a value"
      ref=$2; shift 2 ;;
    --ref=*) ref=${1#--ref=}; shift ;;
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

if [ -n "$from_path" ]; then
  [ -z "$ref" ] || usage_error "--ref cannot be combined with --from-path"
  [ -f "$from_path/Cargo.toml" ] || die "no Cargo.toml in --from-path directory: $from_path"
  case $from_path in
    /*) ;;
    *) from_path="$PWD/$from_path" ;;
  esac
fi

command -v cargo >/dev/null 2>&1 ||
  die "cargo was not found in PATH. Install Rust from https://rustup.rs and run this script again."

if [ -n "$from_path" ]; then
  printf 'Installing cursorswap from %s into %s\n' "$from_path" "$prefix"
  cargo install --path "$from_path" --locked --force --root "$prefix"
else
  command -v git >/dev/null 2>&1 || die "git was not found in PATH; it is required to install from GitHub."
  printf 'Installing cursorswap from %s into %s\n' "$REPO_URL" "$prefix"
  if [ -z "$ref" ]; then
    cargo install --git "$REPO_URL" --locked --force --root "$prefix"
  else
    case $ref in
      v[0-9]*) cargo install --git "$REPO_URL" --tag "$ref" --locked --force --root "$prefix" ;;
      *) cargo install --git "$REPO_URL" --branch "$ref" --locked --force --root "$prefix" ;;
    esac
  fi
fi

bin="$prefix/bin/cursorswap"
[ -x "$bin" ] || die "installation finished but $bin was not found"

printf '\nInstalled: %s\n' "$("$bin" --version)"
printf 'Binary:    %s\n' "$bin"

case ":${PATH:-}:" in
  *":$prefix/bin:"*) ;;
  *)
    printf '\nNote: %s is not in your PATH.\n' "$prefix/bin"
    printf 'Add this line to your shell profile, then restart your shell:\n'
    printf '  export PATH="%s:$PATH"\n' "$prefix/bin"
    ;;
esac
