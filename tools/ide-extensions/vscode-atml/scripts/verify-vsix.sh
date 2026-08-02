#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <vsix> <bundled-server-name>" >&2
  exit 2
fi

artifact=$1
server_name=$2
if [[ ! -f "$artifact" ]]; then
  echo "VSIX not found: $artifact" >&2
  exit 2
fi

entries=$(unzip -Z1 "$artifact")
required="extension/bin/$server_name"
if ! grep -Fxq "$required" <<<"$entries"; then
  echo "missing bundled server: $required" >&2
  exit 1
fi

server_count=$(grep -Ec '^extension/bin/atml-language-server(\.exe)?$' <<<"$entries")
if [[ "$server_count" -ne 1 ]]; then
  echo "expected exactly one bundled language server, found $server_count" >&2
  exit 1
fi

forbidden='(^|/)(src|test|scripts|node_modules|\.vscode-test)/|\.map$|package-lock\.json$|(^|/)(\.env[^/]*|\.npmrc|credentials?[^/]*|secrets?[^/]*|[^/]*\.(pem|key))$'
if grep -Eiq "$forbidden" <<<"$entries"; then
  echo "VSIX contains a forbidden source, cache, lock, or secret-like path" >&2
  grep -Ei "$forbidden" <<<"$entries" >&2
  exit 1
fi

size=$(wc -c < "$artifact")
if [[ "$size" -gt 52428800 ]]; then
  echo "VSIX exceeds 50 MiB: $size bytes" >&2
  exit 1
fi

echo "verified $artifact ($size bytes)"
