import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'character_link.dart';
import 'dictionary.dart';
import 'pitch.dart';
import 'rust/api.dart' hide word;
import 'rust/api.dart' as api show word;
import 'search_screen.dart';
import 'tooltip.dart';

/// Opens the page of a word.
void openWordPage(NavigatorState navigator, int id) {
  navigator.push(
    MaterialPageRoute<void>(builder: (context) => WordPage(id: id)),
  );
}

/// The page of a word: its written forms with their readings, then its senses.
class WordPage extends StatefulWidget {
  const WordPage({super.key, required this.id});

  final int id;

  @override
  State<WordPage> createState() => _WordPageState();
}

class _WordPageState extends State<WordPage> {
  late final Future<Word?> _word = api.word(
    dictionary: DictionaryScope.of(context),
    id: widget.id,
  );

  @override
  Widget build(BuildContext context) {
    return PageScaffold(
      child: PageLoader<Word>(
        future: _word,
        missing: 'This word is not in the dictionary.',
        // The written forms with their readings, a line, then the senses, then the example
        // sentences. The page has no headword: the forms show it.
        builder: (context, word) => SingleChildScrollView(
          padding: pagePadding,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              FormTable(
                word: word,
                fontSize: formFontSize,
                besideFirstReading: FrequencyRank(word: word),
              ),
              const Divider(height: 32),
              SenseList(senses: word.senses),
              SentenceList(wordId: word.id, pageSize: _sentencePage),
            ],
          ),
        ),
      ),
    );
  }
}

/// The number of example sentences of a page: the word page shows 1 page, and "More…" adds the
/// next.
const int _sentencePage = 10;

/// The font size of the written forms and readings, on a word page and on a kanji page.
const double formFontSize = 20;

/// The padding of the content of a tab.
const EdgeInsets pagePadding = EdgeInsets.fromLTRB(16, 8, 16, 24);

/// The frame of a word page or a kanji page: the back arrow and the search icon at the top.
class PageScaffold extends StatelessWidget {
  const PageScaffold({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        actions: [
          IconButton(
            tooltip: 'Search',
            icon: const Icon(Icons.search),
            onPressed: () => openSearchScreen(context),
          ),
        ],
      ),
      body: child,
    );
  }
}

/// A header, then a row of tabs, with the content of the selected tab below them. Each tab
/// scrolls by itself. A page with no tab is the header alone.
class TabbedPage extends StatelessWidget {
  const TabbedPage({super.key, required this.header, required this.tabs});

  final Widget header;

  /// The content of each tab, by its label.
  final Map<String, Widget> tabs;

  @override
  Widget build(BuildContext context) {
    final header = Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
      child: this.header,
    );
    if (tabs.isEmpty) {
      return Align(alignment: Alignment.topLeft, child: header);
    }
    return DefaultTabController(
      length: tabs.length,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          header,
          TabBar(
            isScrollable: true,
            tabAlignment: TabAlignment.start,
            tabs: [for (final label in tabs.keys) Tab(text: label)],
          ),
          // Space below the line of the tab bar, before the content of the tab.
          const SizedBox(height: 8),
          Expanded(child: TabBarView(children: tabs.values.toList())),
        ],
      ),
    );
  }
}

/// Waits for the data of a page, and then builds the page.
class PageLoader<T extends Object> extends StatelessWidget {
  const PageLoader({
    super.key,
    required this.future,
    required this.missing,
    required this.builder,
  });

  final Future<T?> future;

  /// The message for a page with no data.
  final String missing;

  final Widget Function(BuildContext context, T data) builder;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<T?>(
      future: future,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return Center(child: Text('${snapshot.error}'));
        }
        // A query takes a few milliseconds, so the page shows no progress indicator.
        if (snapshot.connectionState != ConnectionState.done) {
          return const SizedBox.expand();
        }
        final data = snapshot.data;
        if (data == null) {
          return Center(child: Text(missing));
        }
        return builder(context, data);
      },
    );
  }
}

/// A word entry in few lines, for the kanji page: its written forms with their readings, with a
/// link to its word page at their right, then its senses. The word page has the rest: the
/// frequency, the example sentences, and what the word page adds after them.
class CondensedEntry extends StatelessWidget {
  const CondensedEntry({super.key, required this.word, this.pageCharacter});

  final Word word;

  /// The kanji of the page that shows the entry.
  final String? pageCharacter;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 4),
          child: FormTable(
            word: word,
            fontSize: formFontSize,
            pageCharacter: pageCharacter,
            trailing: TextButton.icon(
              onPressed: () => openWordPage(Navigator.of(context), word.id),
              style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
              iconAlignment: IconAlignment.end,
              icon: const Icon(Icons.arrow_forward, size: 18),
              label: const Text('Word page'),
            ),
          ),
        ),
        SenseList(senses: word.senses),
      ],
    );
  }
}

/// The readings of a word, each on its own line, with the written forms that it reads on the next
/// line, indented by 1 em: つちくれ with 土塊 and 土くれ, and どかい with 土塊. A reading of no form,
/// such as トリ of 鳥, has no line of forms. The forms of a reading are in brackets, which are in a
/// lighter colour: 【土塊・土くれ】. Each kanji of a form is a character link.
///
/// A reading or a form with tags has a superscript number for each tag, and a legend below the
/// forms gives the description of each number. A number stands for 1 tag, wherever it is: 2 forms
/// that are rK have the same number. Numbers go in the order that the tags first appear.
class FormTable extends StatelessWidget {
  const FormTable({
    super.key,
    required this.word,
    required this.fontSize,
    this.pageCharacter,
    this.trailing,
    this.besideFirstReading,
  });

  final Word word;
  final double fontSize;

  /// The kanji of the page that shows the table. Its link does not open the page again.
  final String? pageCharacter;

  /// A small widget at the top right of the table, in the space that the readings leave, so that
  /// it adds no line: the frequency on a word page, the link to the word page on a kanji page.
  final Widget? trailing;

  /// A small widget after the first reading, on its line, so that it adds no line: the frequency
  /// on a word page.
  final Widget? besideFirstReading;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final style = TextStyle(fontSize: fontSize, height: 1.2);
    // The colour of a divider.
    final mark = style.copyWith(color: colors.outlineVariant);
    final superscript = TextStyle(
      fontSize: math.max(fontSize / 2, _smallestKana),
      height: 1.2,
      color: colors.onSurfaceVariant,
    );

    final label = Theme.of(context).textTheme.bodySmall!
        .copyWith(color: colors.onSurfaceVariant);
    final legend = label.copyWith(color: _softNoteColor(colors));

    Widget pitch(Reading reading, Accent accent) => PitchText(
      morae: reading.morae,
      downstep: accent.downstep,
      style: style,
      // The colour of a divider, as the brackets and the ・ of the written forms.
      lineColor: colors.outlineVariant,
    );

    // The forms of each reading, in position order.
    List<WrittenForm> formsOf(Reading reading) => [
      for (final form in word.writtenForms)
        if (form.readings.any((r) => r.text == reading.text)) form,
    ];

    // The number of each tag, from 1, in the order that the tags first appear.
    final numbers = <Tag, int>{};
    for (final reading in word.readings) {
      for (final tag in [
        ...reading.tags,
        for (final form in formsOf(reading)) ...form.tags,
      ]) {
        numbers.putIfAbsent(tag, () => numbers.length + 1);
      }
    }

    // A text with the numbers of its tags at the top right. The tooltip of the numbers gives the
    // lines of the legend for them.
    Widget tagged(Widget text, List<Tag> tags) {
      if (tags.isEmpty) return text;
      final sorted = [...tags]
        ..sort((a, b) => numbers[a]!.compareTo(numbers[b]!));
      return Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          text,
          Tooltip(
            positionDelegate: tooltipAboveRight,
            message: [
              for (final tag in sorted)
                '${numbers[tag]}  ${tag.description ?? tag.name}',
            ].join('\n'),
            child: Text(
              sorted.map((tag) => numbers[tag]).join(','),
              style: superscript,
            ),
          ),
        ],
      );
    }

    Widget formItem(WrittenForm form) => tagged(
      LinkedText(
        text: form.text,
        links: {for (final link in form.kanji) link.character: link},
        style: style,
        currentCharacter: pageCharacter,
      ),
      form.tags,
    );

    // A reading with its pitch accents, each drawn on a copy of the reading, with ・ between 2
    // accents. The parts of speech of an accent follow it: ふたつ (line over たつ) adv ・ ふたつ
    // (line over ふた, tick) n. The numbers of the tags of the reading follow the first accent.
    Widget readingLine(Reading reading) {
      if (reading.accents.isEmpty) {
        return tagged(Text(reading.text, style: style), reading.tags);
      }
      return Wrap(
        // Each accent has its line above it, so wrapped accents need about the space of 2 readings.
        runSpacing: _rowGap - 4,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          for (final (index, accent) in reading.accents.indexed)
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (index == 0)
                  tagged(pitch(reading, accent), reading.tags)
                else
                  pitch(reading, accent),
                if (accent.partsOfSpeech.isNotEmpty) ...[
                  const SizedBox(width: 4),
                  Tooltip(
                    positionDelegate: tooltipAboveRight,
                    message: [
                      for (final tag in accent.partsOfSpeech)
                        tag.description ?? tag.name,
                    ].join('; '),
                    child: Text(
                      accent.partsOfSpeech.map((tag) => tag.name).join(', '),
                      style: label,
                    ),
                  ),
                ],
                if (index < reading.accents.length - 1) Text('・', style: mark),
              ],
            ),
        ],
      );
    }

    // The forms in brackets, with ・ between 2 forms: 【一寸・鳥渡】. Each form keeps the mark after
    // it, so that a line does not start with ・ or 】. 【 is outside the wrap, so that each line
    // starts under the first form.
    Widget bracketed(List<WrittenForm> forms) => Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('【', style: mark),
        Flexible(
          child: Wrap(
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              for (final (index, form) in forms.indexed)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    formItem(form),
                    Text(index < forms.length - 1 ? '・' : '】', style: mark),
                  ],
                ),
            ],
          ),
        ),
      ],
    );

    final table = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final (index, reading) in word.readings.indexed) ...[
          if (besideFirstReading case final beside? when index == 0)
            Row(
              children: [
                Flexible(child: readingLine(reading)),
                const SizedBox(width: 16),
                beside,
              ],
            )
          else
            readingLine(reading),
          if (formsOf(reading) case final forms when forms.isNotEmpty)
            Padding(
              padding: EdgeInsets.only(left: fontSize, top: 4),
              child: bracketed(forms),
            ),
          const SizedBox(height: _rowGap),
        ],
        // A little more space than between 2 readings, to set the legend apart.
        if (numbers.isNotEmpty) const SizedBox(height: 8),
        for (final MapEntry(key: tag, value: number) in numbers.entries)
          Text.rich(
            TextSpan(
              children: [
                // The number is a little smaller than the description, with 2 spaces after it.
                TextSpan(
                  text: '$number',
                  style: TextStyle(fontSize: legend.fontSize! * 3 / 4),
                ),
                TextSpan(text: '  ${tag.description ?? tag.name}'),
              ],
            ),
            style: legend,
          ),
        if (numbers.isNotEmpty) const SizedBox(height: 12),
      ],
    );
    final trailing = this.trailing;
    if (trailing == null) return table;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(child: table),
        const SizedBox(width: 16),
        trailing,
      ],
    );
  }
}

/// The frequency rank of a word, small and faint, for the line of the first reading of a word
/// page: the rank up to [commonRank], "rare" past it, where the ranks are noise, and "unranked"
/// for a word that Jiten does not rank. The tooltip gives the rank of each written form, then
/// of each reading whose kana rank counts, each in order of frequency.
class FrequencyRank extends StatelessWidget {
  const FrequencyRank({super.key, required this.word});

  final Word word;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final style = TextStyle(fontSize: 12, color: colors.outline);
    final common = commonRank();
    final rank = word.rank;
    final String label;
    if (rank == null) {
      label = 'unranked';
    } else if (rank <= common) {
      label = _grouped(rank);
    } else {
      label = 'rare';
    }
    // The written forms, then the readings, each in order of frequency.
    int byRank((String, int) a, (String, int) b) => a.$2.compareTo(b.$2);
    final ranked = [
      ...[
        for (final form in word.writtenForms)
          if (form.rank case final rank?) (form.text, rank),
      ]..sort(byRank),
      ...[
        for (final reading in word.readings)
          if (reading.rank case final rank?) (reading.text, rank),
      ]..sort(byRank),
    ];
    final message = [
      'Frequency rank (Jiten Dramas)',
      for (final (text, rank) in ranked) '$text  ${_grouped(rank)}',
    ].join('\n');
    return Tooltip(
      positionDelegate: tooltipAboveRight,
      message: message,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.leaderboard_outlined, size: 14, color: colors.outline),
          const SizedBox(width: 4),
          Text(label, style: style),
        ],
      ),
    );
  }
}

/// A number with a comma between each group of 3 digits: 7,364.
String _grouped(int number) {
  final digits = '$number';
  final buffer = StringBuffer();
  for (var i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 == 0) buffer.write(',');
    buffer.write(digits[i]);
  }
  return buffer.toString();
}

/// The colour of a note, such as the legend of the tags or the note of a sense: the muted colour
/// of the theme, a little nearer the background.
Color _softNoteColor(ColorScheme colors) =>
    Color.lerp(colors.onSurfaceVariant, colors.surface, 0.15)!;

/// The space below each reading and its written forms. It leaves room for the pitch accent line
/// above the next reading.
const double _rowGap = 18;

/// The smallest font size of furigana.
const double _smallestKana = 10;

/// The font size of the source of an example sentence: its id and author.
const double _sourceFontSize = 10;

/// A written form with furigana: the kana of each part above it. A part with no kana, such as
/// okurigana, has an empty line above it, so that all parts line up.
class FuriganaText extends StatelessWidget {
  const FuriganaText({
    super.key,
    required this.parts,
    required this.fontSize,
    this.links = const {},
    this.color,
  });

  final List<FuriganaPart> parts;
  final double fontSize;

  /// The colour of the form, for a background that is not the page, such as a tooltip. The
  /// furigana are a lighter shade of it.
  final Color? color;

  /// The characters that open their kanji page.
  final Map<String, CharacterLink> links;

  @override
  Widget build(BuildContext context) {
    // Furigana are half the size of the form, but not smaller than the size that stays readable.
    final kanaStyle = TextStyle(
      fontSize: math.max(fontSize * 0.5, _smallestKana),
      height: 1.1,
      color:
          color?.withValues(alpha: 0.75) ??
          Theme.of(context).colorScheme.onSurfaceVariant,
    );
    final textStyle = TextStyle(fontSize: fontSize, height: 1.2, color: color);
    return Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        for (final part in parts)
          Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(part.kana ?? '', style: kanaStyle),
              LinkedText(text: part.text, links: links, style: textStyle),
            ],
          ),
      ],
    );
  }
}

/// Items in a row that wraps, with a [VerticalDivider] between 2 items. A line does not start with
/// a divider, because each divider stays with the item before it.
class DividedWrap extends StatelessWidget {
  const DividedWrap({super.key, required this.height, required this.children});

  /// The height of each divider: the font size of the items.
  final double height;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      runSpacing: 4,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        for (final (index, child) in children.indexed)
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              child,
              if (index < children.length - 1)
                SizedBox(
                  height: height,
                  child: const VerticalDivider(width: 24, thickness: 1),
                ),
            ],
          ),
      ],
    );
  }
}

/// The senses of a word, numbered. A part-of-speech tag shows only when it differs from the tags
/// of the sense before.
class SenseList extends StatelessWidget {
  const SenseList({super.key, required this.senses});

  final List<Sense> senses;

  static const _partOfSpeech = 'pos';

  @override
  Widget build(BuildContext context) {
    final rows = <Widget>[];
    var previous = const <String>[];
    for (final (index, sense) in senses.indexed) {
      final partsOfSpeech = [
        for (final tag in sense.tags)
          if (tag.category == _partOfSpeech) tag.name,
      ];
      final isRepeated = _sameNames(partsOfSpeech, previous);
      previous = partsOfSpeech;
      rows.add(
        _SenseRow(
          number: index + 1,
          sense: sense,
          tags: [
            for (final tag in sense.tags)
              if (!(isRepeated && tag.category == _partOfSpeech)) tag,
          ],
        ),
      );
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: rows);
  }

  static bool _sameNames(List<String> a, List<String> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

class _SenseRow extends StatelessWidget {
  const _SenseRow({
    required this.number,
    required this.sense,
    required this.tags,
  });

  final int number;
  final Sense sense;

  /// The tags of the sense that the row shows.
  final List<Tag> tags;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final note = theme.textTheme.bodyMedium?.copyWith(
      color: _softNoteColor(theme.colorScheme),
    );
    final restrictions = [...sense.writtenForms, ...sense.readings];
    // The first line of the glosses has no space above its text, so the tags sit just above it.
    const firstLine = TextHeightBehavior(applyHeightToFirstAscent: false);
    return Padding(
      // More space below a sense than above it, so its tags are nearer its own glosses than the
      // glosses of the sense before.
      padding: const EdgeInsets.only(top: 3, bottom: 9),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // The tags are just above the glosses, so the glosses of each sense start in 1 column.
          if (tags.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(left: _numberWidth),
              child: Wrap(
                spacing: 4,
                runSpacing: 2,
                children: [for (final tag in tags) TagChip(tag: tag)],
              ),
            ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(
                width: _numberWidth,
                child: Text(
                  '$number.',
                  style: theme.textTheme.bodyLarge,
                  textHeightBehavior: firstLine,
                ),
              ),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text.rich(
                      TextSpan(children: _glossSpans(sense.glosses)),
                      style: theme.textTheme.bodyLarge,
                      textHeightBehavior: firstLine,
                    ),
                    if (sense.note case final text?) Text(text, style: note),
                    if (restrictions.isNotEmpty)
                      Text('only ${restrictions.join(', ')}', style: note),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  /// The width of the number of a sense, and the indent of its tags and its glosses.
  static const double _numberWidth = 28;

  /// The glosses, joined with "; ". A literal gloss is in quotes, and an explanation is in
  /// italics.
  static List<InlineSpan> _glossSpans(List<Gloss> glosses) {
    return [
      for (final (index, gloss) in glosses.indexed) ...[
        if (index > 0) const TextSpan(text: '; '),
        switch (gloss.glossType) {
          'lit' => TextSpan(text: '“${gloss.text}”'),
          'expl' => TextSpan(
            text: gloss.text,
            style: const TextStyle(fontStyle: FontStyle.italic),
          ),
          _ => TextSpan(text: gloss.text),
        },
      ],
    ];
  }
}

/// A JMdict tag as a small outlined label. Its description is the tooltip.
class TagChip extends StatelessWidget {
  const TagChip({super.key, required this.tag});

  final Tag tag;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final chip = Container(
      padding: const EdgeInsets.symmetric(horizontal: 5),
      // No fill, a thin outline in the colour of a divider, and sharp corners.
      decoration: BoxDecoration(
        border: Border.all(color: theme.colorScheme.outlineVariant),
      ),
      // A line height of 1.2 keeps the chip low, so a row of tags takes little space.
      child: Text(
        tag.name,
        style: theme.textTheme.labelSmall?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
          height: 1.2,
        ),
      ),
    );
    final description = tag.description;
    return description == null
        ? chip
        : Tooltip(
            message: description,
            positionDelegate: tooltipAboveRight,
            child: chip,
          );
  }
}

/// The example sentences of a word, from the sentence bundles, a page at a time: nothing until
/// the first page loads, nothing if the word has none, and the error if a page fails to load. A
/// "More…" button after the sentences loads the next page, while there is one.
class SentenceList extends StatefulWidget {
  const SentenceList({super.key, required this.wordId, required this.pageSize});

  final int wordId;

  /// The number of sentences of a page.
  final int pageSize;

  @override
  State<SentenceList> createState() => _SentenceListState();
}

class _SentenceListState extends State<SentenceList> {
  final List<Sentence> _sentences = [];

  /// Whether the first page has loaded.
  bool _loaded = false;

  /// Whether a page is loading.
  bool _loading = false;

  /// Whether there are sentences after those of [_sentences].
  bool _more = false;

  Object? _error;

  @override
  void initState() {
    super.initState();
    _loadPage();
  }

  /// Loads the next page, with 1 sentence more than a page, to know whether another page follows.
  Future<void> _loadPage() async {
    setState(() => _loading = true);
    try {
      final page = await sentences(
        dictionary: DictionaryScope.of(context),
        wordId: widget.wordId,
        offset: _sentences.length,
        limit: widget.pageSize + 1,
      );
      if (!mounted) return;
      setState(() {
        _sentences.addAll(page.take(widget.pageSize));
        _more = page.length > widget.pageSize;
        _loaded = true;
        _loading = false;
      });
    } on Object catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _loading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final error = _error;
    if (error != null) {
      return Padding(
        padding: const EdgeInsets.only(top: 16),
        child: Text(
          'The examples could not load: $error',
          style: TextStyle(color: Theme.of(context).colorScheme.error),
        ),
      );
    }
    if (!_loaded || _sentences.isEmpty) {
      return const SizedBox.shrink();
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SizedBox(height: 16),
        Text('Examples', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 8),
        for (final sentence in _sentences)
          Padding(
            padding: const EdgeInsets.only(bottom: 16),
            child: SentenceView(sentence: sentence),
          ),
        if (_more)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(
              // A second press while a page loads does nothing.
              onPressed: _loading ? null : _loadPage,
              child: const Text('More…'),
            ),
          ),
      ],
    );
  }
}

/// The tooltip of a word: its headword with furigana (or with its reading after it, for a headword
/// with no furigana), then its gloss. The word chips of a kanji page and the words of an example
/// sentence show it.
InlineSpan wordTooltip(BuildContext context, WordSummary summary) {
  final reading = summary.reading;
  final headword = summary.furigana.isNotEmpty
      ? WidgetSpan(
          child: FuriganaText(
            parts: summary.furigana,
            fontSize: 16,
            color: Theme.of(context).colorScheme.onInverseSurface,
          ),
        )
      : TextSpan(
          text: '${summary.headword}${reading == null ? '' : '  $reading'}',
        );
  return TextSpan(
    children: [
      headword,
      TextSpan(text: '\n${summary.gloss}'),
    ],
  );
}

/// An example sentence with furigana, then its first translation and its source. Each word of the
/// sentence opens its word page, as a [_SentenceWordLink]. The word of the page is in the accent
/// colour, and is not a link.
class SentenceView extends StatelessWidget {
  const SentenceView({super.key, required this.sentence});

  final Sentence sentence;

  /// The size of the text: the size of the written forms, so that the furigana are half of it,
  /// as furigana are in print, and no smaller than [_smallestKana].
  static const double _fontSize = formFontSize;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    // Proportional widths (the OpenType feature palt) take the space at the sides of kana and
    // punctuation away, as Japanese text sets them; kanji keep their full width.
    const proportional = [FontFeature.enable('palt')];
    final kanaStyle = TextStyle(
      fontSize: math.max(_fontSize * 0.5, _smallestKana),
      height: 1.1,
      color: colors.onSurfaceVariant,
      fontFeatures: proportional,
    );
    const textStyle = TextStyle(
      fontSize: _fontSize,
      height: 1.2,
      fontFeatures: proportional,
    );
    final start = sentence.wordStart;
    final end = sentence.wordEnd;
    bool isPageWord(int from, int to) =>
        start != null && end != null && from < end && to > start;

    // The pieces of the line: a part with furigana stays whole, so its kana stay over it; the
    // other parts break into characters, so the line can wrap anywhere in them.
    final pieces = <({int start, int end, String text, String kana})>[];
    var position = 0;
    for (final part in sentence.furigana) {
      final texts = part.kana == null
          ? part.text.characters.toList()
          : [part.text];
      for (final text in texts) {
        final length = text.characters.length;
        pieces.add((
          start: position,
          end: position + length,
          text: text,
          kana: part.kana != null && text == part.text ? part.kana! : '',
        ));
        position += length;
      }
    }

    // A run of kanji with furigana in 1 word is 1 piece, so its kana stay together over it:
    // べんきょう over 勉強, not べん over 勉 and きょう over 強.
    int? wordOf(({int start, int end, String text, String kana}) p) {
      final index = sentence.words.indexWhere(
        (w) => w.start <= p.start && p.end <= w.end,
      );
      return index < 0 ? null : index;
    }

    final merged = <({int start, int end, String text, String kana})>[];
    for (final p in pieces) {
      final last = merged.lastOrNull;
      if (last != null &&
          last.kana.isNotEmpty &&
          p.kana.isNotEmpty &&
          wordOf(last) == wordOf(p)) {
        merged[merged.length - 1] = (
          start: last.start,
          end: p.end,
          text: last.text + p.text,
          kana: last.kana + p.kana,
        );
      } else {
        merged.add(p);
      }
    }
    pieces
      ..clear()
      ..addAll(merged);

    // Furigana as in print: a piece is as wide as its text, and its kana overhang the plain text
    // around it when they are wider, so the text keeps its spacing. Kana stay a gap away from the
    // kana of the next piece, and start with the text at the start of the sentence. Where the
    // plain text gives too little room, the text of the piece spreads to fit its kana.
    final kanaHeight = kanaStyle.fontSize! * kanaStyle.height!;
    final gap = kanaStyle.fontSize! / 2;
    final base = DefaultTextStyle.of(context).style;
    final scaler = MediaQuery.textScalerOf(context);
    double width(String text, TextStyle style) {
      final painter = TextPainter(
        text: TextSpan(text: text, style: base.merge(style)),
        textDirection: TextDirection.ltr,
        textScaler: scaler,
      )..layout();
      final width = painter.width;
      painter.dispose();
      return width;
    }

    final textWidths = [for (final p in pieces) width(p.text, textStyle)];
    final kanaWidths = [
      for (final p in pieces) p.kana.isEmpty ? 0.0 : width(p.kana, kanaStyle),
    ];
    bool hasKana(int i) => pieces[i].kana.isNotEmpty;
    // The width of the plain pieces from `from` up to the next piece with kana, in the direction
    // `step`, and whether a piece with kana ends them.
    (double, bool) plainRun(int from, int step) {
      var sum = 0.0;
      for (var i = from; i >= 0 && i < pieces.length; i += step) {
        if (hasKana(i)) return (sum, true);
        sum += textWidths[i];
      }
      return (sum, false);
    }

    // For each piece: the space added on each side of its text, and where its kana start.
    final spread = List.filled(pieces.length, 0.0);
    // Space before a piece whose kana would come closer than the gap to the kana before them,
    // as when 2 words with furigana touch: 窮鼠 and 猫.
    final shift = List.filled(pieces.length, 0.0);
    final kanaLeft = List.filled(pieces.length, 0.0);
    final rightOverhang = List.filled(pieces.length, 0.0);
    var lastKana = -1;
    for (var i = 0; i < pieces.length; i++) {
      if (!hasKana(i)) continue;
      final over = kanaWidths[i] - textWidths[i];
      if (lastKana >= 0) {
        final (before, _) = plainRun(i - 1, -1);
        // The kana can start with the text at the latest, and overhang only on the right.
        final kanaStart = over <= 0 ? -over / 2 : 0.0;
        shift[i] = math.max(
          0.0,
          rightOverhang[lastKana] + gap - before - kanaStart,
        );
      }
      if (over <= 0) {
        kanaLeft[i] = -over / 2;
      } else {
        final (before, _) = plainRun(i - 1, -1);
        final (after, kanaAfter) = plainRun(i + 1, 1);
        final leftRoom = lastKana < 0
            ? before
            : before + shift[i] - rightOverhang[lastKana] - gap;
        // The plain text before the next kana keeps what their overhang needs on its side: what
        // does not fit in the plain text after them.
        final next = kanaAfter
            ? List.generate(
                pieces.length,
                (n) => n,
              ).firstWhere((n) => n > i && hasKana(n))
            : -1;
        var nextLeft = 0.0;
        if (next >= 0) {
          final (nextAfter, nextKanaAfter) = plainRun(next + 1, 1);
          final nextRoom = nextKanaAfter ? nextAfter - gap : nextAfter;
          final nextOver = kanaWidths[next] - textWidths[next];
          nextLeft = math.max(0.0, nextOver - math.max(0.0, nextRoom));
        }
        final rightRoom = kanaAfter ? after - gap - nextLeft : after;
        var left = math.min(over / 2, math.max(0.0, leftRoom));
        final right = math.min(over - left, math.max(0.0, rightRoom));
        left = math.min(over - right, math.max(0.0, leftRoom));
        spread[i] = over - left - right;
        kanaLeft[i] = -left;
        rightOverhang[i] = right;
      }
      lastKana = i;
    }

    Widget piece(
      ({int start, int end, String text, String kana}) p,
      TextStyle style,
    ) {
      final index = pieces.indexOf(p);
      final side = spread[index] / 2;
      return Stack(
        clipBehavior: Clip.none,
        children: [
          Padding(
            padding: EdgeInsets.fromLTRB(
              shift[index] + side,
              kanaHeight,
              side,
              0,
            ),
            child: Text(p.text, style: style),
          ),
          if (p.kana.isNotEmpty)
            Positioned(
              top: 0,
              left: shift[index] + kanaLeft[index],
              child: Text(p.kana, style: kanaStyle, softWrap: false),
            ),
        ],
      );
    }

    // The units of the line: the pieces of each word make 1 link, and the line wraps between
    // words. A piece in no word, or across the edge of a word, stays alone.
    final units = <Widget>[];
    var index = 0;
    while (index < pieces.length) {
      final first = pieces[index];
      final word = sentence.words
          .where((w) => w.start <= first.start && first.end <= w.end)
          .firstOrNull;
      if (word == null) {
        final style = isPageWord(first.start, first.end)
            ? textStyle.copyWith(color: colors.primary)
            : textStyle;
        units.add(piece(first, style));
        index++;
        continue;
      }
      final inWord = <({int start, int end, String text, String kana})>[];
      while (index < pieces.length && pieces[index].end <= word.end) {
        inWord.add(pieces[index]);
        index++;
      }
      units.add(
        _SentenceWordLink(
          summary: word.word,
          isPageWord: isPageWord(word.start, word.end),
          style: textStyle,
          builder: (style) => Row(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [for (final p in inWord) piece(p, style)],
          ),
        ),
      );
    }

    final translation = sentence.translations.isEmpty
        ? null
        : sentence.translations.first;
    final source = [
      'Tatoeba #${sentence.id}',
      if (sentence.author != null) sentence.author!,
    ].join(' · ');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Wrap(crossAxisAlignment: WrapCrossAlignment.end, children: units),
        if (translation != null)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(translation.text),
          ),
        // The source is for attribution, so it is small and faint: it does not compete with the
        // sentence and its translation.
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Text(
            source,
            style: TextStyle(fontSize: _sourceFontSize, color: colors.outline),
          ),
        ),
      ],
    );
  }
}

/// A word of an example sentence. It looks like the text around it, and turns the accent colour
/// under the pointer, as a character link does; a tap opens its word page, and its tooltip has the
/// headword and the gloss. The word of the page is always in the accent colour, and a tap does
/// nothing.
class _SentenceWordLink extends StatefulWidget {
  const _SentenceWordLink({
    required this.summary,
    required this.isPageWord,
    required this.style,
    required this.builder,
  });

  final WordSummary summary;
  final bool isPageWord;
  final TextStyle style;

  /// Builds the word with the style of its text.
  final Widget Function(TextStyle style) builder;

  @override
  State<_SentenceWordLink> createState() => _SentenceWordLinkState();
}

class _SentenceWordLinkState extends State<_SentenceWordLink> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final page = widget.isPageWord;
    final style = page || _hovered
        ? widget.style.copyWith(color: colors.primary)
        : widget.style;
    return Tooltip(
      positionDelegate: tooltipAboveRight,
      richMessage: wordTooltip(context, widget.summary),
      waitDuration: const Duration(milliseconds: 300),
      child: MouseRegion(
        cursor: page ? SystemMouseCursors.basic : SystemMouseCursors.click,
        onEnter: (event) => setState(() => _hovered = true),
        onExit: (event) => setState(() => _hovered = false),
        child: GestureDetector(
          onTap: page
              ? null
              : () => openWordPage(Navigator.of(context), widget.summary.id),
          child: widget.builder(style),
        ),
      ),
    );
  }
}
