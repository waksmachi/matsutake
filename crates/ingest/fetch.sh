#!/usr/bin/env bash
# Downloads the sources of the ingest crate into sources/.
# Set KANJIVG_RELEASE to a release tag, for example r20250816, to get a KanjiVG release that is not the
# default.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p sources

kanjivg_release="${KANJIVG_RELEASE:-r20250816}"
kanjivg_file="kanjivg-${kanjivg_release#r}.xml.gz"

sources=(
  "JMdict_e.gz https://www.edrdg.org/pub/Nihongo/JMdict_e.gz"
  "kanjidic2.xml.gz https://www.edrdg.org/kanjidic/kanjidic2.xml.gz"
  "$kanjivg_file https://github.com/KanjiVG/kanjivg/releases/download/$kanjivg_release/$kanjivg_file"
)

rm -f sources/kanjivg-*.xml.gz
for source in "${sources[@]}"; do
  read -r file url <<<"$source"
  echo "Downloading $file" >&2
  curl -fsSL -o "sources/$file" "$url"
done
