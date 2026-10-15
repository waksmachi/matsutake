#!/usr/bin/env bash
# Downloads the sources of the ingest crate into sources/.
# Set KANJIVG_RELEASE to a release tag, for example r20250816, to get a KanjiVG release that is not the
# default. Set KANJIUM_COMMIT to a commit of Kanjium to get its accents.txt from that commit.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p sources

kanjivg_release="${KANJIVG_RELEASE:-r20250816}"
kanjivg_file="kanjivg-${kanjivg_release#r}.xml.gz"

# Kanjium has no releases. The commit is the last change of accents.txt, and names the file.
kanjium_commit="${KANJIUM_COMMIT:-685d4d723d6d20bf9beb169103aeac188eb067ad}"
kanjium_file="kanjium-accents-${kanjium_commit:0:7}.txt"

sources=(
  "JMdict_e.gz https://www.edrdg.org/pub/Nihongo/JMdict_e.gz"
  "kanjidic2.xml.gz https://www.edrdg.org/kanjidic/kanjidic2.xml.gz"
  "$kanjivg_file https://github.com/KanjiVG/kanjivg/releases/download/$kanjivg_release/$kanjivg_file"
  "$kanjium_file https://raw.githubusercontent.com/mifunetoshiro/kanjium/$kanjium_commit/data/source_files/raw/accents.txt"
)

# JmdictFurigana has a release each month. The tag of the release, for example 2.3.1+2026-09-25,
# gives the date in the file name, and ingest reads the version from the file name.
furigana_url="https://github.com/Doublevil/JmdictFurigana/releases/latest"
furigana_tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$furigana_url")"
furigana_date="${furigana_tag##*%2B}"
furigana_date="${furigana_date##*+}"
sources+=("JmdictFurigana-$furigana_date.txt $furigana_url/download/JmdictFurigana.txt")

rm -f sources/kanjivg-*.xml.gz sources/JmdictFurigana-*.txt sources/kanjium-accents-*.txt
for source in "${sources[@]}"; do
  read -r file url <<<"$source"
  echo "Downloading $file" >&2
  curl -fsSL -o "sources/$file" "$url"
done
