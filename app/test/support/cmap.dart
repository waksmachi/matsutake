import 'dart:typed_data';

/// Reads the character map (the `cmap` table) of a font file, and gives each code point that has
/// a glyph.
///
/// The reader takes the Unicode subtables in the formats 4 and 12, which hold the code points of
/// the fonts of the app. It ignores the other subtables, for example the variation sequences of
/// format 14.
Set<int> readCodePoints(Uint8List bytes) {
  final data = ByteData.sublistView(bytes);
  final cmap = _tableOffset(data, 'cmap');
  final codePoints = <int>{};
  final subtableCount = data.getUint16(cmap + 2);
  for (var i = 0; i < subtableCount; i++) {
    final record = cmap + 4 + i * 8;
    final platform = data.getUint16(record);
    final encoding = data.getUint16(record + 2);
    final isUnicode =
        platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10));
    if (!isUnicode) continue;
    final subtable = cmap + data.getUint32(record + 4);
    switch (data.getUint16(subtable)) {
      case 4:
        _readFormat4(data, subtable, codePoints);
      case 12:
        _readFormat12(data, subtable, codePoints);
    }
  }
  return codePoints;
}

/// Gives the offset of a table of the font.
int _tableOffset(ByteData data, String tag) {
  final tableCount = data.getUint16(4);
  for (var i = 0; i < tableCount; i++) {
    final record = 12 + i * 16;
    final name = String.fromCharCodes([
      for (var j = 0; j < 4; j++) data.getUint8(record + j),
    ]);
    if (name == tag) return data.getUint32(record + 8);
  }
  throw FormatException('The font has no $tag table');
}

/// Format 4: segments of code points of the Basic Multilingual Plane.
void _readFormat4(ByteData data, int subtable, Set<int> codePoints) {
  final segmentCount = data.getUint16(subtable + 6) ~/ 2;
  final endCodes = subtable + 14;
  final startCodes = endCodes + segmentCount * 2 + 2;
  final deltas = startCodes + segmentCount * 2;
  final rangeOffsets = deltas + segmentCount * 2;
  for (var i = 0; i < segmentCount; i++) {
    final start = data.getUint16(startCodes + i * 2);
    final end = data.getUint16(endCodes + i * 2);
    final delta = data.getUint16(deltas + i * 2);
    final rangeOffset = data.getUint16(rangeOffsets + i * 2);
    for (var code = start; code <= end && code != 0xFFFF; code++) {
      var glyph = code;
      if (rangeOffset != 0) {
        final address = rangeOffsets + i * 2 + rangeOffset + (code - start) * 2;
        glyph = data.getUint16(address);
        if (glyph == 0) continue;
      }
      if ((glyph + delta) & 0xFFFF != 0) {
        codePoints.add(code);
      }
    }
  }
}

/// Format 12: groups of code points of each plane.
void _readFormat12(ByteData data, int subtable, Set<int> codePoints) {
  final groupCount = data.getUint32(subtable + 12);
  for (var i = 0; i < groupCount; i++) {
    final group = subtable + 16 + i * 12;
    final start = data.getUint32(group);
    final end = data.getUint32(group + 4);
    final startGlyph = data.getUint32(group + 8);
    for (var code = start; code <= end; code++) {
      if (startGlyph + (code - start) != 0) {
        codePoints.add(code);
      }
    }
  }
}
