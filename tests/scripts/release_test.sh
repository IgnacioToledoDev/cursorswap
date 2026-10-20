#!/bin/sh
# Tests for scripts/release.sh (version logic only, no network).
# bump cases run on a throwaway copy of a fake Cargo.toml; the real one is never touched.
set -u

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
REL="$ROOT/scripts/release.sh"

SB=$(mktemp -d "${TMPDIR:-/tmp}/cursorswap-release.XXXXXX") || exit 1
case $SB in
  ""|/) echo "refusing to use sandbox '$SB'" >&2; exit 1 ;;
esac
trap 'rm -rf -- "$SB"' EXIT INT TERM

OUT="$SB/out.txt"
ERR="$SB/err.txt"
failures=0

pass() { printf 'PASS: %s\n' "$1"; }
fail() {
  printf 'FAIL: %s\n' "$1"
  [ -f "$OUT" ] && sed 's/^/      out| /' "$OUT"
  [ -f "$ERR" ] && sed 's/^/      err| /' "$ERR"
  failures=$((failures + 1))
}
run() { # run <args...>: capture stdout and stderr separately, set rc
  sh "$REL" "$@" >"$OUT" 2>"$ERR"
  rc=$?
}
expect_out() { # expect_out <expected stdout>
  [ "$(cat "$OUT")" = "$1" ]
}

# fake project directory with a Cargo.toml
FAKE="$SB/proj"
mkdir -p "$FAKE"
write_cargo() { # write_cargo <version>
  cat >"$FAKE/Cargo.toml" <<TOML
[package]
name = "demo"
version = "$1"
edition = "2024"

[dependencies]
clap = { version = "4.6.7", features = ["derive"] }
semver = "1.0.28"

[dev-dependencies]
version = "9.9.9"
TOML
}
in_fake() { (cd "$FAKE" && sh "$REL" "$@") >"$OUT" 2>"$ERR"; rc=$?; }

if [ -x "$REL" ]; then pass "release.sh is executable"; else fail "release.sh is executable"; fi

# check-tag: valid tags
valid() { # valid <tag> <prerelease>
  run check-tag "$1"
  version=${1#v}
  if [ "$rc" -eq 0 ] && expect_out "version=$version
prerelease=$2"; then pass "check-tag accepts $1"; else fail "check-tag accepts $1"; fi
}
valid v0.2.0 false
valid v1.0.0 false
valid v10.20.30 false
valid v0.2.0-rc.1 true
valid v1.0.0-alpha true
valid v1.0.0-alpha.1 true
valid v1.0.0-0.3.7 true
valid v1.0.0-x.7.z.92 true
valid v1.0.0-alpha-a.b-c true

# check-tag: invalid tags
invalid() { # invalid <tag>
  run check-tag "$1"
  if [ "$rc" -ne 0 ] && [ ! -s "$OUT" ] && grep -q '^error:' "$ERR"; then
    pass "check-tag rejects '$1'"
  else fail "check-tag rejects '$1'"; fi
}
invalid 0.2.0
invalid V0.2.0
invalid v0.2
invalid v1
invalid v01.0.0
invalid v1.02.0
invalid v1.0.0-01
invalid v1.0.0-
invalid v1.0.0-rc..1
invalid v1.0.0+build
invalid v1.0.0-rc.1+build
invalid "v1.0.0 "
invalid " v1.0.0"
invalid 'v1.0.0;rm'
# The literal $(id) is the test input: it must be rejected, never expanded.
# shellcheck disable=SC2016
invalid 'v1.0.0$(id)'
invalid "v1.0.0
v2.0.0"
invalid ""
invalid vlatest

run check-tag 'v1.0.0+build'
if grep -qi 'build metadata' "$ERR"; then pass "build metadata rejection is explained"; else fail "build metadata rejection is explained"; fi
run check-tag
if [ "$rc" -eq 2 ]; then pass "check-tag without argument exits 2"; else fail "check-tag without argument exits 2"; fi

# cargo-version
write_cargo 0.1.0
in_fake cargo-version
if [ "$rc" -eq 0 ] && expect_out "0.1.0"; then pass "cargo-version prints the package version"; else fail "cargo-version prints the package version"; fi
rm "$FAKE/Cargo.toml"
in_fake cargo-version
if [ "$rc" -ne 0 ]; then pass "cargo-version fails without Cargo.toml"; else fail "cargo-version fails without Cargo.toml"; fi

# verify-tag was removed: the tag, not Cargo.toml, defines the release version
write_cargo 0.2.0
in_fake verify-tag v0.2.0
if [ "$rc" -eq 2 ] && grep -q 'unknown command' "$ERR" && [ ! -s "$OUT" ]; then pass "verify-tag is no longer a command"; else fail "verify-tag is no longer a command"; fi
run --help
if ! grep -q 'verify-tag' "$OUT"; then pass "--help does not mention verify-tag"; else fail "--help does not mention verify-tag"; fi
# check-tag does not depend on Cargo.toml: a tag ahead of it is valid
in_fake check-tag v0.3.0
if [ "$rc" -eq 0 ] && expect_out "version=0.3.0
prerelease=false"; then pass "check-tag ignores the Cargo.toml version"; else fail "check-tag ignores the Cargo.toml version"; fi

# bump
write_cargo 0.1.0
cp "$FAKE/Cargo.toml" "$SB/before.toml"
in_fake bump 0.2.0
sed 's/^version = "0.2.0"$/version = "0.1.0"/' "$FAKE/Cargo.toml" >"$SB/restored.toml"
if [ "$rc" -eq 0 ] && grep -q '0\.1\.0' "$OUT" && grep -q '0\.2\.0' "$OUT" \
  && [ "$(grep -c '^version = "0.2.0"$' "$FAKE/Cargo.toml")" -eq 1 ] \
  && cmp -s "$SB/before.toml" "$SB/restored.toml" \
  && grep -q '^version = "9.9.9"$' "$FAKE/Cargo.toml" \
  && grep -q 'clap = { version = "4.6.7"' "$FAKE/Cargo.toml"; then
  pass "bump rewrites only the package version line"
else fail "bump rewrites only the package version line"; fi

in_fake bump 0.2.0
if [ "$rc" -ne 0 ] && grep -q '^error:' "$ERR"; then pass "bump refuses the same version"; else fail "bump refuses the same version"; fi

bump_refused() { # bump_refused <version>
  write_cargo 0.1.0
  cp "$FAKE/Cargo.toml" "$SB/before.toml"
  in_fake bump "$1"
  if [ "$rc" -ne 0 ] && cmp -s "$SB/before.toml" "$FAKE/Cargo.toml"; then
    pass "bump refuses '$1' and leaves Cargo.toml intact"
  else fail "bump refuses '$1' and leaves Cargo.toml intact"; fi
}
bump_refused v0.2.0
bump_refused 0.2
bump_refused 01.0.0
bump_refused 1.0.0+build
bump_refused 1.0.0-rc..1
bump_refused ""
bump_refused '1.0.0;rm'

write_cargo 0.1.0
in_fake bump 0.3.0-rc.1
if [ "$rc" -eq 0 ] && grep -q '^version = "0.3.0-rc.1"$' "$FAKE/Cargo.toml"; then pass "bump accepts a pre-release version"; else fail "bump accepts a pre-release version"; fi

# bump touches only the Cargo.toml of the current directory
EMPTY="$SB/empty"
mkdir -p "$EMPTY"
(cd "$EMPTY" && sh "$REL" bump 0.2.0) >"$OUT" 2>"$ERR"
rc=$?
if [ "$rc" -ne 0 ] && [ ! -e "$EMPTY/Cargo.toml" ]; then pass "bump fails without Cargo.toml"; else fail "bump fails without Cargo.toml"; fi

# alpha-version: next patch after the latest stable tag plus -alpha.<counter>
alpha_ok() { # alpha_ok <tag> <counter> <expected version>
  run alpha-version "$1" "$2"
  if [ "$rc" -eq 0 ] && expect_out "$3"; then pass "alpha-version $1 $2 -> $3"; else fail "alpha-version $1 $2 -> $3"; fi
}
alpha_ok v0.3.2 7 0.3.3-alpha.7
alpha_ok v1.0.0 1 1.0.1-alpha.1
alpha_ok v0.9.9 42 0.9.10-alpha.42
alpha_ok v10.20.30 120 10.20.31-alpha.120

alpha_refused() { # alpha_refused <tag> <counter>
  run alpha-version "$1" "$2"
  if [ "$rc" -ne 0 ] && [ ! -s "$OUT" ] && grep -q '^error:' "$ERR"; then
    pass "alpha-version rejects '$1' '$2'"
  else fail "alpha-version rejects '$1' '$2'"; fi
}
alpha_refused v0.3.2-rc.1 1
alpha_refused v0.3.3-alpha.1 2
alpha_refused 0.3.2 1
alpha_refused v0.3 1
alpha_refused v1.0.0+build 1
alpha_refused "" 1
alpha_refused vlatest 1
alpha_refused v0.3.2 ""
alpha_refused v0.3.2 abc
alpha_refused v0.3.2 -1
alpha_refused v0.3.2 1.5
alpha_refused v0.3.2 01
alpha_refused v0.3.2 '1;rm'
alpha_refused v0.3.2 "1
2"
run alpha-version v0.3.2
if [ "$rc" -eq 2 ]; then pass "alpha-version with one argument exits 2"; else fail "alpha-version with one argument exits 2"; fi
run alpha-version
if [ "$rc" -eq 2 ]; then pass "alpha-version without arguments exits 2"; else fail "alpha-version without arguments exits 2"; fi
run alpha-version v0.3.2 1 extra
if [ "$rc" -eq 2 ]; then pass "alpha-version with three arguments exits 2"; else fail "alpha-version with three arguments exits 2"; fi

# usage
run --help
if [ "$rc" -eq 0 ] && grep -q 'check-tag' "$OUT" && grep -q 'bump' "$OUT" && grep -q 'alpha-version' "$OUT"; then pass "--help"; else fail "--help"; fi
run -h
if [ "$rc" -eq 0 ]; then pass "-h"; else fail "-h"; fi
run frobnicate
if [ "$rc" -eq 2 ] && grep -q -i 'usage' "$ERR"; then pass "unknown subcommand exits 2"; else fail "unknown subcommand exits 2"; fi
run
if [ "$rc" -eq 2 ]; then pass "no subcommand exits 2"; else fail "no subcommand exits 2"; fi

if [ "$failures" -eq 0 ]; then
  echo "all release script checks passed"
else
  printf '%s release script check(s) failed\n' "$failures"
  exit 1
fi
