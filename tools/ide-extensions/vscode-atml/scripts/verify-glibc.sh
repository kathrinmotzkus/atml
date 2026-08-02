#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <binary> <maximum-glibc-version>" >&2
  exit 2
fi

binary=$1
maximum=$2
if [[ ! -f "$binary" ]]; then
  echo "binary not found: $binary" >&2
  exit 2
fi

required=$(
  LANG=C readelf -W --version-info --dyn-syms "$binary" |
    grep -oE 'GLIBC_[0-9]+\.[0-9]+' |
    sed 's/^GLIBC_//' |
    sort -Vu |
    tail -n 1
)

if [[ -z "$required" ]]; then
  echo "could not determine the minimum glibc version for $binary" >&2
  exit 1
fi

oldest=$(printf '%s\n%s\n' "$required" "$maximum" | sort -Vu | head -n 1)
if [[ "$oldest" != "$required" ]]; then
  echo "$binary requires glibc $required, exceeding the allowed maximum $maximum" >&2
  exit 1
fi

echo "verified $binary requires glibc $required (maximum $maximum)"
