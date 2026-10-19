"""Writes lib/src/kana_ink.dart: the left and right edges of the ink of each kana of Noto Sans CJK
JP, as fractions of the font size from the start of its box. The pitch accent line uses them to
put a fall in pitch halfway between the ink of 2 kana.

Usage: python3 tool/kana_ink.py

The script needs fontTools, as tool/subset_fallback.sh does.
"""

from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.ttLib import TTFont

APP = Path(__file__).resolve().parent.parent
FONT = APP / "assets/fonts/NotoSansCJKjp-Regular.otf"
OUT = APP / "lib/src/kana_ink.dart"

# Hiragana, katakana, and the marks of both: ぁ to ゖ, ゝ and ゞ, ァ to ヺ, and ー, ヽ, and ヾ.
KANA = [
    *range(0x3041, 0x3097),
    0x309D,
    0x309E,
    *range(0x30A1, 0x30FB),
    0x30FC,
    0x30FD,
    0x30FE,
]


def main():
    font = TTFont(FONT)
    cmap = font.getBestCmap()
    glyphs = font.getGlyphSet()
    em = font["head"].unitsPerEm
    lines = []
    for code in KANA:
        name = cmap.get(code)
        if name is None:
            continue
        pen = BoundsPen(glyphs)
        glyphs[name].draw(pen)
        if pen.bounds is None:
            continue
        left, _, right, _ = pen.bounds
        lines.append(f"  0x{code:04X}: ({left / em:.3f}, {right / em:.3f}), // {chr(code)}")
    OUT.write_text(
        "// Written by tool/kana_ink.py from NotoSansCJKjp-Regular.otf. Do not edit.\n"
        "\n"
        "/// The left and right edges of the ink of each kana, as fractions of the font size from\n"
        "/// the start of its box.\n"
        "const kanaInk = <int, (double, double)>{\n" + "\n".join(lines) + "\n};\n"
    )


if __name__ == "__main__":
    main()
