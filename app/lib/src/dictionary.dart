import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';

import 'rust/api.dart';

const _asset = 'assets/content.db';
const _fileName = 'content.db';

/// Opens the dictionary of the app.
///
/// SQLite cannot open a file inside the asset bundle. So on the first launch, this function copies
/// `content.db` to the application support directory. Each launch opens that copy.
Future<Dictionary> openBundledDictionary() async {
  final directory = await getApplicationSupportDirectory();
  final file = File('${directory.path}/$_fileName');
  if (!await file.exists()) {
    await directory.create(recursive: true);
    final data = await rootBundle.load(_asset);
    // A copy that stops halfway must not leave a content.db, so the copy has another name until
    // it is complete.
    final partial = File('${file.path}.tmp');
    await partial.writeAsBytes(
      data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
      flush: true,
    );
    await partial.rename(file.path);
  }
  return openDictionary(path: file.path);
}

/// Gives the dictionary to the pages below it.
class DictionaryScope extends InheritedWidget {
  const DictionaryScope({
    super.key,
    required this.dictionary,
    required super.child,
  });

  final Dictionary dictionary;

  /// The dictionary of the nearest scope. A caller can use it in `initState`, because the
  /// dictionary of a scope does not change.
  static Dictionary of(BuildContext context) {
    final scope = context.getInheritedWidgetOfExactType<DictionaryScope>();
    assert(scope != null, 'No DictionaryScope above this widget');
    return scope!.dictionary;
  }

  @override
  bool updateShouldNotify(DictionaryScope oldWidget) =>
      dictionary != oldWidget.dictionary;
}
