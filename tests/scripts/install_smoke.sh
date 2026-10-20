#!/bin/sh
# Smoke test for install.sh and uninstall.sh.
# Everything runs in a throwaway sandbox; the real HOME and XDG dirs are never touched.
set -u

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
INSTALL="$ROOT/install.sh"
UNINSTALL="$ROOT/uninstall.sh"

# Keep the real toolchain and cargo cache reachable after HOME is replaced.
CARGO_HOME=${CARGO_HOME:-${HOME:-}/.cargo}
RUSTUP_HOME=${RUSTUP_HOME:-${HOME:-}/.rustup}
export CARGO_HOME RUSTUP_HOME

SB=$(mktemp -d "${TMPDIR:-/tmp}/cursorswap-smoke.XXXXXX") || exit 1
case $SB in
  ""|/) echo "refusing to use sandbox '$SB'" >&2; exit 1 ;;
esac
trap 'rm -rf -- "$SB"' EXIT INT TERM

HOME="$SB/home"
XDG_CONFIG_HOME="$HOME/.config"
XDG_DATA_HOME="$HOME/.local/share"
PREFIX="$SB/prefix"
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$SB/target}
export HOME XDG_CONFIG_HOME XDG_DATA_HOME CARGO_TARGET_DIR
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME/icons"

BIN="$PREFIX/bin/cursorswap"
CFG="$XDG_CONFIG_HOME/cursorswap"
OUT="$SB/out.txt"
failures=0

pass() { printf 'PASS: %s\n' "$1"; }
fail() {
  printf 'FAIL: %s\n' "$1"
  [ -f "$OUT" ] && sed 's/^/      | /' "$OUT"
  failures=$((failures + 1))
}
check() { # check <description> <command...>
  desc=$1
  shift
  if "$@" >/dev/null 2>&1; then pass "$desc"; else fail "$desc"; fi
}
run() { # run <command...>: capture combined output, set rc
  "$@" >"$OUT" 2>&1
  rc=$?
}
out_has() { grep -q -- "$1" "$OUT"; }
out_hasi() { grep -qi -- "$1" "$OUT"; }
absent() { [ ! -e "$1" ] && [ ! -L "$1" ]; }

check "install.sh is executable" test -x "$INSTALL"
check "uninstall.sh is executable" test -x "$UNINSTALL"

# --help
run sh "$INSTALL" --help
if [ "$rc" -eq 0 ] && out_has '--prefix' && out_has '--from-path'; then
  pass "install.sh --help"
else fail "install.sh --help"; fi
run sh "$UNINSTALL" --help
if [ "$rc" -eq 0 ] && out_has '--purge-config' && out_has '--keep-config'; then
  pass "uninstall.sh --help"
else fail "uninstall.sh --help"; fi

# Unknown option
run sh "$INSTALL" --bogus
if [ "$rc" -ne 0 ]; then pass "install.sh rejects unknown option"; else fail "install.sh rejects unknown option"; fi

# Guard clauses
for bad in "" "/" "//"; do
  run sh "$INSTALL" --prefix "$bad" --from-path "$ROOT"
  if [ "$rc" -ne 0 ] && out_hasi 'prefix'; then
    pass "install.sh rejects prefix '$bad'"
  else fail "install.sh rejects prefix '$bad'"; fi
  run sh "$UNINSTALL" --prefix "$bad" --keep-config
  if [ "$rc" -ne 0 ] && out_hasi 'prefix'; then
    pass "uninstall.sh rejects prefix '$bad'"
  else fail "uninstall.sh rejects prefix '$bad'"; fi
done

# Missing cargo
EMPTY="$SB/empty-path"
mkdir -p "$EMPTY"
run env PATH="$EMPTY" /bin/sh "$INSTALL" --prefix "$PREFIX" --from-path "$ROOT"
if [ "$rc" -ne 0 ] && out_hasi 'cargo' && absent "$BIN"; then
  pass "install.sh fails clearly without cargo"
else fail "install.sh fails clearly without cargo"; fi

# Install from the local checkout, then re-run
run sh "$INSTALL" --prefix "$PREFIX" --from-path "$ROOT"
if [ "$rc" -eq 0 ] && [ -x "$BIN" ]; then pass "install from local checkout"; else fail "install from local checkout"; fi
if [ -x "$BIN" ] && "$BIN" --version 2>/dev/null | grep -q cursorswap; then
  pass "installed binary reports its version"
else fail "installed binary reports its version"; fi
if out_has "$PREFIX/bin"; then pass "install.sh mentions PATH for a prefix outside PATH"; else fail "install.sh mentions PATH for a prefix outside PATH"; fi

run sh "$INSTALL" --prefix "$PREFIX" --from-path "$ROOT"
if [ "$rc" -eq 0 ] && [ -x "$BIN" ]; then pass "re-running install is idempotent"; else fail "re-running install is idempotent"; fi

# Registry plus a theme symlink that uninstall must report but keep
THEME="$SB/themes/Demo-cursors"
mkdir -p "$THEME" "$CFG/conf"
printf '[[cursor]]\nname = "Demo-cursors"\npath = "%s"\n' "$THEME" >"$CFG/conf/registry.toml"
ln -s "$THEME" "$XDG_DATA_HOME/icons/Demo-cursors"
mkdir -p "$SB/themes/Other"
ln -s "$SB/themes/Other" "$XDG_DATA_HOME/icons/Other"

# Uninstall, keeping the config
run sh "$UNINSTALL" --prefix "$PREFIX" --keep-config
if [ "$rc" -eq 0 ] && absent "$BIN" && [ -f "$CFG/conf/registry.toml" ]; then
  pass "uninstall --keep-config removes binary, keeps config"
else fail "uninstall --keep-config removes binary, keeps config"; fi
if out_has "$XDG_DATA_HOME/icons/Demo-cursors" && ! out_has "icons/Other"; then
  pass "uninstall lists only registered theme symlinks"
else fail "uninstall lists only registered theme symlinks"; fi
if [ -L "$XDG_DATA_HOME/icons/Demo-cursors" ] && [ -d "$THEME" ]; then
  pass "uninstall leaves theme symlinks and directories alone"
else fail "uninstall leaves theme symlinks and directories alone"; fi

# No flag and no terminal: config is kept, with a note
run sh "$UNINSTALL" --prefix "$PREFIX" </dev/null
if [ "$rc" -eq 0 ] && [ -f "$CFG/conf/registry.toml" ] && out_hasi 'purge-config'; then
  pass "uninstall without flags and without a terminal keeps config"
else fail "uninstall without flags and without a terminal keeps config"; fi

# Purge, after the binary is already gone
run sh "$UNINSTALL" --prefix "$PREFIX" --purge-config
if [ "$rc" -eq 0 ] && absent "$CFG"; then pass "uninstall --purge-config removes config"; else fail "uninstall --purge-config removes config"; fi
if [ -d "$XDG_CONFIG_HOME" ]; then pass "purge does not remove XDG_CONFIG_HOME itself"; else fail "purge does not remove XDG_CONFIG_HOME itself"; fi

# Idempotent uninstall
run sh "$UNINSTALL" --prefix "$PREFIX" --keep-config
if [ "$rc" -eq 0 ]; then pass "uninstall twice exits 0"; else fail "uninstall twice exits 0"; fi

# Uninstall without cargo falls back to removing only the binary
run sh "$INSTALL" --prefix "$PREFIX" --from-path "$ROOT"
touch "$PREFIX/bin/unrelated"
TOOLS="$SB/tools"
mkdir -p "$TOOLS"
for t in rm ls sed grep cat dirname basename mkdir readlink tr; do
  p=$(command -v "$t") && ln -s "$p" "$TOOLS/$t"
done
run env PATH="$TOOLS" /bin/sh "$UNINSTALL" --prefix "$PREFIX" --keep-config
if [ "$rc" -eq 0 ] && absent "$BIN" && [ -e "$PREFIX/bin/unrelated" ]; then
  pass "uninstall without cargo removes only the binary"
else fail "uninstall without cargo removes only the binary"; fi

echo
if [ "$failures" -eq 0 ]; then
  echo "All checks passed."
  exit 0
fi
echo "$failures check(s) failed."
exit 1
