#!/usr/bin/env bash
set -euo pipefail

if [[ ! -f Cargo.lock ]]; then
  echo "configulator: no Cargo.lock in $PWD" >&2
  exit 1
fi
version=$(awk '/^name = "configulator-rs"$/ { found = 1; next } found && /^version = / { gsub(/"/, "", $3); print $3; exit }' Cargo.lock)
if [[ -z $version ]]; then
  echo "configulator: configulator-rs is not in Cargo.lock" >&2
  exit 1
fi

root=${XDG_CACHE_HOME:-$HOME/.cache}/configulator-cli/$version
if [[ ! -x $root/bin/configulator ]]; then
  cargo install --quiet --locked --root "$root" --version "=$version" configulator-cli
fi
exec "$root/bin/configulator" "$@"
