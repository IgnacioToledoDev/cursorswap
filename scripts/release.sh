#!/bin/sh
# Release helper: SemVer validation for release tags and Cargo.toml version bumps.
# Pure logic, no network. Run from the repository root.
set -eu

# Semantic Versioning 2.0.0 (semver.org regex) without the build-metadata group.
# Build metadata is rejected on purpose: the Rust `semver` crate orders versions
# by it, unlike the specification, so release versions must not carry any.
SEMVER_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-((0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*))?$'
NL='
'

usage() {
  cat <<'USAGE'
Usage: release.sh <command> [argument]

Commands:
  check-tag <tag>       validate a release tag (vX.Y.Z[-pre]); print version= and prerelease=
  cargo-version         print the package version from ./Cargo.toml
  verify-tag <tag>      check-tag, then require the tag to match ./Cargo.toml
  bump <version>        set the package version in ./Cargo.toml (no leading "v")
  -h, --help            show this help
USAGE
}

die() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

# validate_version <version>: exit 1 with a message unless it is plain SemVer 2.0.0.
validate_version() {
  case $1 in
    *"$NL"*) die "version must be a single line" ;;
    *+*) die "build metadata ('+...') is not allowed in release versions" ;;
  esac
  printf '%s\n' "$1" | grep -Eq "$SEMVER_RE" ||
    die "'$1' is not a valid Semantic Versioning 2.0.0 version (expected X.Y.Z or X.Y.Z-prerelease)"
}

# tag_version <tag>: validate the tag and set VERSION.
tag_version() {
  case $1 in
    v*) ;;
    *) die "tag '$1' must start with a lowercase 'v'" ;;
  esac
  VERSION=${1#v}
  validate_version "$VERSION"
}

cargo_version() {
  [ -f Cargo.toml ] || die "Cargo.toml not found in the current directory"
  v=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
  [ -n "$v" ] || die "no package version found in Cargo.toml"
  printf '%s\n' "$v"
}

print_tag_info() {
  printf 'version=%s\n' "$VERSION"
  case $VERSION in
    *-*) echo 'prerelease=true' ;;
    *) echo 'prerelease=false' ;;
  esac
}

need_arg() {
  [ "$1" -eq 1 ] || { usage >&2; exit 2; }
}

[ "$#" -ge 1 ] || { usage >&2; exit 2; }
cmd=$1
shift

case $cmd in
  -h | --help)
    usage
    ;;
  check-tag)
    need_arg "$#"
    tag_version "$1"
    print_tag_info
    ;;
  cargo-version)
    need_arg "$(($# + 1))"
    cargo_version
    ;;
  verify-tag)
    need_arg "$#"
    tag_version "$1"
    current=$(cargo_version)
    [ "$VERSION" = "$current" ] ||
      die "tag $1 does not match Cargo.toml version $current"
    print_tag_info
    ;;
  bump)
    need_arg "$#"
    validate_version "$1"
    new=$1
    old=$(cargo_version)
    [ "$new" != "$old" ] || die "Cargo.toml is already at version $old"
    tmp=$(mktemp "${TMPDIR:-/tmp}/release-bump.XXXXXX")
    trap 'rm -f -- "$tmp"' EXIT INT TERM
    NEW=$new awk '
      !done && /^version = ".*"$/ { print "version = \"" ENVIRON["NEW"] "\""; done = 1; next }
      { print }
    ' Cargo.toml >"$tmp"
    cat "$tmp" >Cargo.toml
    printf 'old=%s\nnew=%s\n' "$old" "$new"
    ;;
  *)
    printf 'error: unknown command: %s\n' "$cmd" >&2
    usage >&2
    exit 2
    ;;
esac
