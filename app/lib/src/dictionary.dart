import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';

import 'rust/api.dart';

const _asset = 'assets/content.db';
const _fileName = 'content.db';

/// The folder of the sentence bundles that ship with the app. The app copies each to the folder
/// of bundles. The folder can be empty: a build of the app needs no bundle.
const _bundleAssets = 'assets/bundles/';

/// The folder of sentence bundles, in the application support directory. The app attaches each
/// `.db` file of it, so a bundle that a user adds there joins the dictionary on the next launch.
const _bundleFolder = 'bundles';

/// Opens the dictionary of the app, with its sentence bundles.
///
/// SQLite cannot open a file inside the asset bundle. So on the first launch, this function copies
/// `content.db` and the bundled sentence bundles to the application support directory. Each launch
/// opens those copies.
Future<Dictionary> openBundledDictionary() async {
  final directory = await getApplicationSupportDirectory();
  final file = File('${directory.path}/$_fileName');
  await _copyAsset(_asset, file);
  final dictionary = await openDictionary(path: file.path);

  final bundles = Directory('${directory.path}/$_bundleFolder');
  final manifest = await AssetManifest.loadFromAssetBundle(rootBundle);
  for (final asset in manifest.listAssets()) {
    if (asset.startsWith(_bundleAssets) && asset.endsWith('.db')) {
      await _copyAsset(asset, File('${bundles.path}/${asset.split('/').last}'));
    }
  }
  if (!await bundles.exists()) {
    return dictionary;
  }
  final files =
      bundles
          .listSync()
          .whereType<File>()
          .where((file) => file.path.endsWith('.db'))
          .toList()
        ..sort((a, b) => a.path.compareTo(b.path));
  for (final bundle in files) {
    try {
      final info = await attachBundle(
        dictionary: dictionary,
        path: bundle.path,
      );
      debugPrint('Attached the bundle ${info.id} (${info.version})');
    } on Object catch (error) {
      // A file that is not a bundle, or of another version, stays out; the others still load.
      debugPrint('Cannot attach ${bundle.path}: $error');
    }
  }
  return dictionary;
}

/// Copies the asset `asset` to `file`, unless `file` exists.
Future<void> _copyAsset(String asset, File file) async {
  if (await file.exists()) {
    return;
  }
  await file.parent.create(recursive: true);
  final data = await rootBundle.load(asset);
  // A copy that stops halfway must not leave the file, so the copy has another name until it is
  // complete.
  final partial = File('${file.path}.tmp');
  await partial.writeAsBytes(
    data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
    flush: true,
  );
  await partial.rename(file.path);
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
