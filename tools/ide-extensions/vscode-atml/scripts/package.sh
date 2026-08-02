#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <vsce-target> <language-server-binary>" >&2
  exit 2
fi

vsce_target=$1
server_source=$2
case "$vsce_target" in
  linux-x64|linux-arm64|win32-x64|win32-arm64|darwin-x64|darwin-arm64) ;;
  *) echo "unsupported VS Code target: $vsce_target" >&2; exit 2 ;;
esac

if [[ ! -f "$server_source" ]]; then
  echo "language server not found: $server_source" >&2
  exit 2
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
extension_dir=$(cd -- "$script_dir/.." && pwd)
cd "$extension_dir"

server_name=atml-language-server
if [[ "$vsce_target" == win32-* ]]; then
  server_name=atml-language-server.exe
fi

mkdir -p bin dist
cp "$server_source" "bin/$server_name"
chmod 755 "bin/$server_name" 2>/dev/null || true

version=$(node -p "require('./package.json').version")
artifact="dist/atml-${version}-${vsce_target}.vsix"
npx --no-install vsce package --no-dependencies --target "$vsce_target" --out "$artifact"
source_date_epoch=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}
node "$script_dir/normalize-vsix.cjs" "$artifact" "$source_date_epoch"
"$script_dir/verify-vsix.sh" "$artifact" "$server_name"
(cd dist && sha256sum "$(basename "$artifact")") > "$artifact.sha256"
echo "$artifact"
