import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'src/dictionary.dart';
import 'src/rust/api.dart';
import 'src/search_screen.dart';
import 'src/rust/frb_generated.dart';

const _mainFont = 'Noto Sans CJK JP';

/// Holds only the characters of content.db that the main font lacks.
const _fallbackFont = 'Matsutake Fallback';

const _japanese = Locale('ja');

/// The licence of each font of the app, with the name of the font.
const _fontLicences = {
  'Noto Sans CJK JP': 'assets/fonts/NotoSansCJKjp-LICENSE.txt',
  'Plangothic': 'assets/fonts/fallback-LICENSE.txt',
};

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  LicenseRegistry.addLicense(_licences);
  runApp(const MatsutakeApp());
}

Stream<LicenseEntry> _licences() async* {
  for (final MapEntry(key: font, value: asset) in _fontLicences.entries) {
    yield LicenseEntryWithLineBreaks([
      font,
    ], await rootBundle.loadString(asset));
  }
}

/// The app: it opens the dictionary, and then shows the search screen.
class MatsutakeApp extends StatefulWidget {
  const MatsutakeApp({super.key});

  @override
  State<MatsutakeApp> createState() => _MatsutakeAppState();
}

class _MatsutakeAppState extends State<MatsutakeApp> {
  final Future<Dictionary> _dictionary = openBundledDictionary();
  SearchSession? _session;

  @override
  void dispose() {
    _session?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<Dictionary>(
      future: _dictionary,
      builder: (context, snapshot) {
        final dictionary = snapshot.data;
        final Widget home;
        if (snapshot.hasError) {
          home = _Message('Cannot open the dictionary: ${snapshot.error}');
        } else if (dictionary == null) {
          // The first launch copies content.db, which takes a moment.
          home = const _Message('Opening the dictionary…');
        } else {
          home = const SearchScreen();
        }
        final app = MaterialApp(
          title: 'Matsutake',
          debugShowCheckedModeBanner: false,
          theme: buildTheme(Brightness.light),
          darkTheme: buildTheme(Brightness.dark),
          home: home,
        );
        if (dictionary == null) return app;
        // The search screen is the first screen. Each page that the user opens goes on top of
        // it, and the search icon of a page opens it again.
        return DictionaryScope(
          dictionary: dictionary,
          child: SearchScope(
            session: _session ??= SearchSession(dictionary),
            child: app,
          ),
        );
      },
    );
  }
}

/// The theme of the app. All text uses the fonts of the app, with the locale `ja`, so a kanji has
/// its Japanese shape on each device. The locale of the app stays English.
ThemeData buildTheme(Brightness brightness) {
  final theme = ThemeData(
    colorScheme: ColorScheme.fromSeed(
      seedColor: const Color(0xFF8D6E63),
      brightness: brightness,
      // Vermilion, the accent of the app: the selected tab, the focused field, and the
      // highlighted entry. A deep shade on the light background and a bright one on the dark,
      // each with a contrast of 4.5:1 or more with the background.
      primary: brightness == Brightness.light
          ? const Color(0xFFB8341F)
          : const Color(0xFFFF6F4F),
      onPrimary: brightness == Brightness.light
          ? const Color(0xFFFFFFFF)
          : const Color(0xFF561F0A),
      // The dividers, the brackets of the written forms, and the pitch accent lines. On the dark
      // background the colour of the seed (#53433E) is a little faint, so it is a little lighter.
      outlineVariant: brightness == Brightness.dark
          ? const Color(0xFF5E4D48)
          : null,
    ),
    fontFamily: _mainFont,
    fontFamilyFallback: const [_fallbackFont],
  );
  return theme.copyWith(
    textTheme: _withLocale(theme.textTheme),
    primaryTextTheme: _withLocale(theme.primaryTextTheme),
  );
}

TextTheme _withLocale(TextTheme theme) {
  TextStyle? japanese(TextStyle? style) => style?.copyWith(locale: _japanese);
  return TextTheme(
    displayLarge: japanese(theme.displayLarge),
    displayMedium: japanese(theme.displayMedium),
    displaySmall: japanese(theme.displaySmall),
    headlineLarge: japanese(theme.headlineLarge),
    headlineMedium: japanese(theme.headlineMedium),
    headlineSmall: japanese(theme.headlineSmall),
    titleLarge: japanese(theme.titleLarge),
    titleMedium: japanese(theme.titleMedium),
    titleSmall: japanese(theme.titleSmall),
    bodyLarge: japanese(theme.bodyLarge),
    bodyMedium: japanese(theme.bodyMedium),
    bodySmall: japanese(theme.bodySmall),
    labelLarge: japanese(theme.labelLarge),
    labelMedium: japanese(theme.labelMedium),
    labelSmall: japanese(theme.labelSmall),
  );
}

class _Message extends StatelessWidget {
  const _Message(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Scaffold(body: Center(child: Text(text)));
  }
}
