#!/usr/bin/env bash
# Downloads the Tatoeba exports that the tatoeba binary reads into sources/tatoeba-YYYY-MM-DD/,
# and decompresses them. Tatoeba exports its data each week. The date of the export of the
# Japanese sentences names the folder, and is the version of the bundle.
set -euo pipefail

cd "$(dirname "$0")"
base="https://downloads.tatoeba.org/exports"
files=(
  per_language/jpn/jpn_sentences_detailed.tsv.bz2
  per_language/jpn/jpn_transcriptions.tsv.bz2
  per_language/jpn/jpn-eng_links.tsv.bz2
  per_language/eng/eng_sentences_detailed.tsv.bz2
  jpn_indices.tar.bz2
)

modified="$(curl -fsSI "$base/${files[0]}" | tr -d '\r' | sed -n 's/^last-modified: //Ip')"
date="$(date -j -u -f '%a, %d %b %Y %H:%M:%S GMT' "$modified" +%Y-%m-%d 2>/dev/null ||
  date -u -d "$modified" +%Y-%m-%d)"
dir="sources/tatoeba-$date"
mkdir -p "$dir"

for file in "${files[@]}"; do
  name="$(basename "$file")"
  echo "Downloading $name" >&2
  curl -fsSL -o "$dir/$name" "$base/$file"
  case "$name" in
    *.tar.bz2) tar -xjf "$dir/$name" -C "$dir" && rm "$dir/$name" ;;
    *.bz2) bunzip2 -f "$dir/$name" ;;
  esac
done
echo "$dir" >&2
