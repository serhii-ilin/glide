#!/usr/bin/env bash
# Re-fetch vendored WPT files listed in tests/compat/wpt/MANIFEST.txt.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WPT_DIR="$ROOT/tests/compat/wpt"
MANIFEST="$WPT_DIR/MANIFEST.txt"
COMMIT="$(sed -n 's/^# commit: //p' "$MANIFEST" | head -1)"
BASE="https://raw.githubusercontent.com/web-platform-tests/wpt/${COMMIT}"

if [[ -z "$COMMIT" ]]; then
  echo "MANIFEST.txt must start with '# commit: <sha>'" >&2
  exit 1
fi

rewrite_html() {
  local path="$1"
  local file="$2"
  local depth
  depth="$(echo "$path" | tr -cd '/' | wc -c | tr -d ' ')"
  local rel=""
  local i=0
  while [[ "$i" -lt "$depth" ]]; do
    rel="../$rel"
    i=$((i + 1))
  done
  if [[ ! -f "$file" ]]; then
    return
  fi
  sed -i.bak \
    -e "s|=\"/resources/|=\"${rel}resources/|g" \
    -e "s|=\"/fonts/|=\"${rel}fonts/|g" \
    -e "s|=\"/css/support/|=\"${rel}css/support/|g" \
    -e "s|href=\"/resources/|href=\"${rel}resources/|g" \
    -e "s|href=\"/fonts/|href=\"${rel}fonts/|g" \
  "$file"
  rm -f "${file}.bak"
  if grep -q 'testharnessreport.js' "$file"; then
    sed -i.bak "s|testharnessreport.js|glide-testharnessreport.js|g" "$file"
    rm -f "${file}.bak"
  fi
}

while IFS= read -r line || [[ -n "$line" ]]; do
  line="${line%%#*}"
  line="$(echo "$line" | xargs)"
  [[ -z "$line" ]] && continue
  dest="$WPT_DIR/$line"
  mkdir -p "$(dirname "$dest")"
  if [[ "$line" == integration/* ]]; then
    continue
  fi
  if [[ -f "$dest" && "$line" == resources/glide-testharnessreport.js ]]; then
    continue
  fi
  {
    url="$BASE/$line"
    echo "fetch $line"
    curl -fsSL "$url" -o "$dest"
    if [[ "$line" == *.html ]]; then
      rewrite_html "$line" "$dest"
    fi
  }
done < "$MANIFEST"

LICENSE_URL="$BASE/LICENSE.md"
curl -fsSL "$LICENSE_URL" -o "$WPT_DIR/LICENSE.md"
echo "Vendored WPT at commit $COMMIT"