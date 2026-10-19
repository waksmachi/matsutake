import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:sqlite3/sqlite3.dart';

import 'support/cmap.dart';

const _database = 'assets/content.db';
const _regular = 'assets/fonts/NotoSansCJKjp-Regular.otf';
const _bold = 'assets/fonts/NotoSansCJKjp-Bold.otf';
const _fallback = 'assets/fonts/fallback.ttf';

Set<int> _codePoints(String path) =>
    readCodePoints(File(path).readAsBytesSync());

/// Characters of content.db that no font of the app has: Latin letters with marks in glosses,
/// such as č and ṣ, and a few symbols. The system draws them, with a font of its own.
const _systemCharacters = {
  0x0105, 0x0107, 0x010D, 0x0159, 0x015A, 0x015B, 0x015F, 0x0161, 0x017E, //
  0x0219, 0x03D5, 0x1E43, 0x1E63, 0x2252, 0x2300,
};

/// Gives the code point of each `character` row of content.db, and of each character of each
/// text column.
Set<int> _characters() {
  final database = sqlite3.open(_database, mode: OpenMode.readOnly);
  try {
    final characters = {
      for (final row in database.select('SELECT id FROM character'))
        row['id'] as int,
    };
    final tables = database.select(
      "SELECT name FROM sqlite_master WHERE type = 'table'",
    );
    for (final table in tables) {
      final name = table['name'] as String;
      for (final column in database.select('PRAGMA table_info($name)')) {
        if ((column['type'] as String).toUpperCase() != 'TEXT') continue;
        final columnName = column['name'] as String;
        final rows = database.select(
          'SELECT DISTINCT $columnName AS text FROM $name '
          'WHERE $columnName IS NOT NULL',
        );
        for (final row in rows) {
          characters.addAll((row['text'] as String).runes);
        }
      }
    }
    return characters;
  } finally {
    database.close();
  }
}

/// Lists each code point with its character, 1 for each line.
String _list(Iterable<int> codePoints) {
  final sorted = codePoints.toList()..sort();
  return sorted
      .map((code) {
        final hex = code.toRadixString(16).toUpperCase().padLeft(4, '0');
        return '${String.fromCharCode(code)} U+$hex';
      })
      .join('\n');
}

void main() {
  // The test compares code points, and does not draw text. It needs the content.db of ingest.
  final skip = File(_database).existsSync()
      ? null
      : '$_database does not exist. Run ingest with --out app/assets.';

  group('fonts', skip: skip, () {
    late final Set<int> characters = _characters();
    late final Set<int> regular = _codePoints(_regular);
    late final Set<int> fallback = _codePoints(_fallback);

    test('each character has a glyph', () {
      final missing = characters
          .difference(regular)
          .difference(fallback)
          .difference(_systemCharacters);
      expect(
        missing,
        isEmpty,
        reason:
            'No font of the app has a glyph for these characters. Run '
            'tool/subset_main_font.py with the full fonts of Noto Sans CJK JP, '
            'then tool/subset_fallback.sh with the characters that it does not '
            'cover:\n${_list(missing)}',
      );

      final notInBold = regular.difference(_codePoints(_bold));
      expect(
        notInBold,
        isEmpty,
        reason:
            'The Bold weight lacks these characters of the Regular weight:\n'
            '${_list(notInBold)}',
      );
    });

    test('fallback font has no unused glyph', () {
      final unused = fallback.difference(characters.difference(regular));
      expect(
        unused,
        isEmpty,
        reason:
            'fallback.ttf holds these characters, which are not characters of '
            'content.db that Noto Sans CJK JP lacks:\n${_list(unused)}',
      );
    });
  });
}
