#!/bin/sh
# Checks that every version string of a release agrees: the Cargo package version, center2 in
# Cargo.lock, and `*! version` and `local c2_version` in stata/center2.ado (the ado compares the
# latter with the plugin's version).
# Usage: scripts/check-versions.sh [vX.Y.Z]   (with a tag argument it must equal v<version>)
# Prints the version on success; exits 1 on any mismatch.
set -eu
cd "$(dirname "$0")/.."

cargo_version=$(awk '
  /^\[package\]/ { inpkg = 1; next }
  /^\[/ { inpkg = 0 }
  inpkg && /^version *=/ { gsub(/.*= *"|".*/, ""); print; exit }
' Cargo.toml)
if [ -z "$cargo_version" ]; then
  echo "could not read [package] version from Cargo.toml" >&2
  exit 1
fi

status=0
check() { # name value
  if [ "$2" != "$cargo_version" ]; then
    echo "version mismatch: $1 is '$2', Cargo.toml version is '$cargo_version'" >&2
    status=1
  fi
}

check "stata/center2.ado *! version" "$(sed -n 's/^\*! version \([^ ,]*\).*/\1/p' stata/center2.ado | head -n 1)"
check "stata/center2.ado local c2_version" \
  "$(sed -n 's/^[[:space:]]*local c2_version[[:space:]]*\([^[:space:]]*\).*/\1/p' stata/center2.ado)"
check "Cargo.lock center2" "$(awk '
  /^\[\[package\]\]/ { name = "" }
  /^name = / { name = $3; gsub(/"/, "", name) }
  /^version = / && name == "center2" { v = $3; gsub(/"/, "", v); print v; exit }
' Cargo.lock)"
if [ $# -ge 1 ]; then
  check "tag $1 (without the leading v)" "${1#v}"
  case "$1" in v*) ;; *) echo "tag '$1' does not start with v" >&2; status=1 ;; esac
fi

[ $status -eq 0 ] || exit 1
echo "$cargo_version"
