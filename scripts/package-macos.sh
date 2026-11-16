#!/bin/bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 SIGNED_APP OUTPUT_DMG SIGNING_IDENTITY" >&2
  exit 2
fi

app=$1
image=$2
identity=$3
repo_dir=$(cd "$(dirname "$0")/.." && pwd)

test ! -e "$image"
codesign --verify --deep --strict "$app"

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/License Notices"
ditto "$app" "$stage/Quick Look STEP.app"
ln -s /Applications "$stage/Applications"
cp "$repo_dir/INSTALL.md" "$stage/Install Quick Look STEP.txt"
cp "$repo_dir/LICENSE" "$stage/License Notices/Quick Look STEP MIT.txt"
cp "$repo_dir/foxtrot/LICENSE-MIT" "$stage/License Notices/Foxtrot MIT.txt"
cp "$repo_dir/foxtrot/LICENSE-APACHE" "$stage/License Notices/Foxtrot Apache 2.0.txt"

hdiutil create -volname "Quick Look STEP" -format UDZO -srcfolder "$stage" "$image"
codesign --sign "$identity" --timestamp "$image"
codesign --verify --strict "$image"
hdiutil verify "$image"
