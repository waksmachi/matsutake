"""Makes assets/fonts/NotoSansCJKjp-Regular.otf and NotoSansCJKjp-Bold.otf from the full fonts of
Noto Sans CJK JP: the glyphs of the characters that the app can show, and no others.

Usage: python3 tool/subset_main_font.py NotoSansCJKjp-Regular.otf NotoSansCJKjp-Bold.otf

The arguments are the full fonts, from Sans/OTF/Japanese of https://github.com/notofonts/noto-cjk.
The script needs fontTools, as tool/subset_fallback.sh does, and assets/content.db. Run it again
when test/font_coverage_test.dart finds a character of content.db with no glyph.

The fonts keep:
- each character of each text column of content.db, and each `character` row
- printable ASCII and Latin-1, for the English of the app and of the glosses
- the hiragana, katakana, CJK punctuation, and fullwidth forms, which a query can hold
- each character of the strings of lib/
"""

import sqlite3
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

APP = Path(__file__).resolve().parent.parent
DATABASE = APP / "assets/content.db"
OUT = APP / "assets/fonts"

BLOCKS = [
    range(0x0020, 0x007F),  # printable ASCII
    range(0x00A0, 0x0100),  # Latin-1
    range(0x2000, 0x2070),  # general punctuation, such as … and ‘’
    range(0x3000, 0x3100),  # CJK punctuation, hiragana, and katakana
    range(0x31F0, 0x3200),  # small katakana
    range(0xFF00, 0xFFF0),  # fullwidth and halfwidth forms
]


def database_code_points():
    database = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    points = {row[0] for row in database.execute("SELECT id FROM character")}
    tables = [row[0] for row in database.execute("SELECT name FROM sqlite_master WHERE type = 'table'")]
    for table in tables:
        for column in database.execute(f"PRAGMA table_info({table})"):
            if column[2].upper() != "TEXT":
                continue
            for (text,) in database.execute(
                f"SELECT DISTINCT {column[1]} FROM {table} WHERE {column[1]} IS NOT NULL"
            ):
                points.update(ord(c) for c in text)
    return points


def source_code_points():
    points = set()
    for path in (APP / "lib").rglob("*.dart"):
        points.update(ord(c) for c in path.read_text(encoding="utf-8"))
    return points


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    points = database_code_points() | source_code_points()
    for block in BLOCKS:
        points.update(block)
    for source, name in zip(sys.argv[1:], ["NotoSansCJKjp-Regular.otf", "NotoSansCJKjp-Bold.otf"]):
        font = TTFont(source)
        kept = points & set(font.getBestCmap())
        options = subset.Options()
        options.layout_features = ["*"]
        options.name_IDs = ["*"]
        options.name_languages = ["*"]
        options.notdef_outline = True
        subsetter = subset.Subsetter(options)
        subsetter.populate(unicodes=kept)
        subsetter.subset(font)
        font.save(OUT / name)
        print(f"{name}: {len(kept)} characters, {(OUT / name).stat().st_size} bytes")


if __name__ == "__main__":
    main()
