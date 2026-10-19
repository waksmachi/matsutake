#!/usr/bin/env bash
# Makes assets/fonts/fallback.ttf from Plangothic P1: the characters of content.db that Noto Sans
# CJK JP lacks.
#
# Usage: tool/subset_fallback.sh PlangothicP1-Regular.ttf CHARACTER...
#
# Each CHARACTER is a code point, for example U+20A7A, or the character itself. The new font holds
# these characters and the characters that fallback.ttf already holds. The script needs pyftsubset
# and ttx of fontTools.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 PlangothicP1-Regular.ttf CHARACTER..." >&2
  exit 2
fi

source_font="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
shift
cd "$(dirname "$0")/.."
out=assets/fonts/fallback.ttf

unicodes=()
text=""
for character in "$@"; do
  if [[ $character == U+* ]]; then
    unicodes+=("$character")
  else
    text+="$character"
  fi
done
if [[ -f $out ]]; then
  while read -r code; do
    unicodes+=("U+$code")
  done < <(ttx -q -t cmap -o - "$out" | grep -o 'code="0x[0-9a-fA-F]*"' | sed 's/code="0x//; s/"//' | sort -u)
fi

# The font has no layout features and no hints, because the app draws 1 character at a time from
# it. A character that Plangothic P1 lacks stops the script.
pyftsubset "$source_font" \
  --unicodes="$(IFS=,; echo "${unicodes[*]-}")" \
  --text="$text" \
  --no-ignore-missing-unicodes \
  --layout-features='' \
  --no-hinting \
  --output-file="$out.tmp"
mv "$out.tmp" "$out"
echo "Wrote $out" >&2
