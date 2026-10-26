import 'package:flutter/material.dart';

import 'character_link.dart';
import 'dictionary.dart';
import 'rust/api.dart' hide character;
import 'rust/api.dart' as api show character;
import 'word_page.dart';
import 'tooltip.dart';

/// The page asks for each word of the kanji, because the Words tab scrolls.
const _wordLimit = 1 << 20;

/// The number of on readings and of kun readings in the header, in order of word count.
const _readingsOfEachKind = 3;

/// The number of meanings in the header.
const _meanings = 5;

/// Opens the page of a kanji.
void openKanjiPage(NavigatorState navigator, String character) {
  navigator.push(
    MaterialPageRoute<void>(
      builder: (context) => KanjiPage(character: character),
    ),
  );
}

/// The page of a kanji: the kanji and its first readings at the top, then the tabs Dict, Chars,
/// and Words.
class KanjiPage extends StatefulWidget {
  const KanjiPage({super.key, required this.character});

  final String character;

  @override
  State<KanjiPage> createState() => _KanjiPageState();
}

class _KanjiPageState extends State<KanjiPage> {
  late final Future<Kanji?> _kanji = api.character(
    dictionary: DictionaryScope.of(context),
    character: widget.character,
    wordLimit: _wordLimit,
  );

  @override
  Widget build(BuildContext context) {
    return PageScaffold(
      child: PageLoader<Kanji>(
        future: _kanji,
        missing: 'This character is not in the dictionary.',
        builder: (context, kanji) => TabbedPage(
          header: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // The frequency rank is under the kanji, in the space that the readings leave.
              Column(
                children: [
                  SelectableText(
                    kanji.character,
                    style: const TextStyle(fontSize: 64, height: 1.2),
                  ),
                  RankLabel(
                    label: switch (kanji.rank) {
                      final rank? => grouped(rank),
                      null => 'unranked',
                    },
                    tooltip: 'Frequency rank (Tatoeba)',
                  ),
                ],
              ),
              const SizedBox(width: 24),
              Expanded(child: _KanjiDetails(kanji: kanji)),
            ],
          ),
          // A tab with nothing in it is hidden.
          tabs: {
            if (kanji.entries.isNotEmpty) 'Dict': _DictTab(kanji: kanji),
            if (kanji.wordCount > 0) 'Words': _WordsTab(kanji: kanji),
          },
        ),
      ),
    );
  }
}

/// The readings and the meanings of a kanji, for the header: the on readings on 1 line and the
/// kun readings on the next, each with its label, then the meanings.
class _KanjiDetails extends StatelessWidget {
  const _KanjiDetails({required this.kanji});

  final Kanji kanji;

  @override
  Widget build(BuildContext context) {
    List<CharacterReading> ofKind(ReadingKind kind) => kanji.readings
        .where((reading) => reading.kind == kind)
        .take(_readingsOfEachKind)
        .toList();
    final on = ofKind(ReadingKind.on_);
    final kun = ofKind(ReadingKind.kun);
    final meanings = kanji.meanings.take(_meanings);
    final lines = [
      if (on.isNotEmpty) ('On', _Readings(readings: on)),
      if (kun.isNotEmpty) ('Kun', _Readings(readings: kun)),
      if (kanji.components.isNotEmpty)
        ('Components', _Components(links: kanji.components)),
    ];
    final theme = Theme.of(context);
    final label = theme.textTheme.labelMedium?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // The labels are 1 column, so the lines start at the same place.
          Table(
            columnWidths: const {
              0: IntrinsicColumnWidth(),
              1: FlexColumnWidth(),
            },
            defaultVerticalAlignment: TableCellVerticalAlignment.middle,
            children: [
              for (final (text, content) in lines)
                TableRow(
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(right: 12),
                      child: Text(text, style: label),
                    ),
                    Padding(
                      padding: const EdgeInsets.only(bottom: 2),
                      child: content,
                    ),
                  ],
                ),
            ],
          ),
          if (meanings.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: _Meanings(meanings: meanings.toList()),
            ),
        ],
      ),
    );
  }
}

/// The components of a kanji, each a character that opens its kanji page.
class _Components extends StatelessWidget {
  const _Components({required this.links});

  final List<CharacterLink> links;

  @override
  Widget build(BuildContext context) {
    return DividedWrap(
      height: 18,
      children: [
        for (final link in links)
          CharacterLinkText(
            link: link,
            style: const TextStyle(fontSize: 18, height: 1.2),
          ),
      ],
    );
  }
}

/// The meanings, with a divider between 2 meanings.
class _Meanings extends StatelessWidget {
  const _Meanings({required this.meanings});

  final List<String> meanings;

  @override
  Widget build(BuildContext context) {
    final style = Theme.of(context).textTheme.bodyLarge;
    return DividedWrap(
      height: style?.fontSize ?? 16,
      children: [for (final meaning in meanings) Text(meaning, style: style)],
    );
  }
}

/// Readings of 1 kind. A kun reading shows its okurigana in a lighter colour, and keeps the `-`
/// of a prefix or a suffix.
class _Readings extends StatelessWidget {
  const _Readings({required this.readings});

  final List<CharacterReading> readings;

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return DividedWrap(
      height: 18,
      children: [
        for (final reading in readings)
          Text.rich(switch (reading.text.split('.')) {
            [final stem, final okurigana] => TextSpan(
              children: [
                TextSpan(text: stem),
                TextSpan(
                  text: okurigana,
                  style: TextStyle(color: muted),
                ),
              ],
            ),
            _ => TextSpan(text: reading.text),
          }, style: const TextStyle(fontSize: 18, height: 1.2)),
      ],
    );
  }
}

/// The Dict tab of a kanji page: the entry of each word that is the kanji, 1 after the other, the
/// most frequent first. Each entry links to its word page, which has the whole of the word.
class _DictTab extends StatelessWidget {
  const _DictTab({required this.kanji});

  final Kanji kanji;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return SingleChildScrollView(
      padding: pagePadding,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (index, entry) in kanji.entries.indexed) ...[
            if (index > 0) Divider(height: 24, color: colors.outlineVariant),
            CondensedEntry(word: entry, pageCharacter: kanji.character),
          ],
        ],
      ),
    );
  }
}

/// The Words tab of a kanji page: the other words that contain the kanji, as chips. The sub-tabs
/// split the words by the position of the kanji: at the start, at the end, or inside.
class _WordsTab extends StatefulWidget {
  const _WordsTab({required this.kanji});

  final Kanji kanji;

  @override
  State<_WordsTab> createState() => _WordsTabState();
}

class _WordsTabState extends State<_WordsTab> {
  int _tab = 0;

  @override
  Widget build(BuildContext context) {
    final kanji = widget.kanji;
    final character = kanji.character;
    final lists = [
      if (kanji.words.starting.isNotEmpty)
        ('$character…', kanji.words.starting),
      if (kanji.words.ending.isNotEmpty) ('…$character', kanji.words.ending),
      if (kanji.words.inside.isNotEmpty) ('…$character…', kanji.words.inside),
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(
            pagePadding.left,
            pagePadding.top,
            pagePadding.right,
            16,
          ),
          child: _SubTabs(
            labels: [
              for (final (label, words) in lists) '$label ${words.length}',
            ],
            selected: _tab,
            onSelected: (tab) => setState(() => _tab = tab),
          ),
        ),
        Expanded(
          child: _WordChips(key: ValueKey(_tab), words: lists[_tab].$2),
        ),
      ],
    );
  }
}

/// Small tabs inside a tab. 1 tab is selected.
class _SubTabs extends StatelessWidget {
  const _SubTabs({
    required this.labels,
    required this.selected,
    required this.onSelected,
  });

  final List<String> labels;
  final int selected;
  final ValueChanged<int> onSelected;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Wrap(
      spacing: 6,
      runSpacing: 6,
      children: [
        for (final (index, label) in labels.indexed)
          Material(
            color: index == selected
                ? colors.secondaryContainer
                : colors.surfaceContainerLow,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(16),
              side: BorderSide(color: colors.outlineVariant),
            ),
            clipBehavior: Clip.antiAlias,
            child: InkWell(
              onTap: () => onSelected(index),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 4,
                ),
                child: Text(
                  label,
                  style: theme.textTheme.labelLarge?.copyWith(
                    color: index == selected
                        ? colors.onSecondaryContainer
                        : colors.onSurfaceVariant,
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// A grid of word chips that builds only the chips in view, because a kanji can have thousands
/// of words. The words are in order of length, where kana do not count: 鳥かご has length 1, as
/// 鳥 has. Then they are in order of their whole length, and each whole length has its own grid,
/// whose columns fit the words. A line separates the words of each number of kanji, and the
/// number is in a narrow column at the left of its words.
class _WordChips extends StatelessWidget {
  const _WordChips({super.key, required this.words});

  final List<WordSummary> words;

  static const double _fontSize = 16;
  static const double _chipHeight = 30;
  static const double _chipPadding = 8;
  static const double _gap = 6;

  /// The width of the column of the number of kanji of each group.
  static const double _countWidth = 20;

  @override
  Widget build(BuildContext context) {
    // The words of each number of kanji are 1 group. Each run of words of a group with the same
    // length with kana is 1 grid.
    final groups = <List<List<WordSummary>>>[];
    for (final word in words) {
      if (groups.isEmpty || _length(groups.last.first.first) != _length(word)) {
        groups.add([[]]);
      } else if (_width(groups.last.last.first) != _width(word)) {
        groups.last.add([]);
      }
      groups.last.last.add(word);
    }
    final colors = Theme.of(context).colorScheme;
    final countStyle = Theme.of(context).textTheme.labelSmall
        ?.copyWith(color: colors.onSurfaceVariant);
    return LayoutBuilder(
      builder: (context, constraints) {
        final width =
            constraints.maxWidth - pagePadding.horizontal - _countWidth;
        return CustomScrollView(
          slivers: [
            for (final (index, group) in groups.indexed) ...[
              SliverPadding(
                padding: EdgeInsets.fromLTRB(
                  pagePadding.left,
                  0,
                  pagePadding.right,
                  index == groups.length - 1 ? pagePadding.bottom : 0,
                ),
                // The number of kanji in a narrow column at the left of the group, level with
                // its first row.
                sliver: SliverCrossAxisGroup(
                  slivers: [
                    SliverConstrainedCrossAxis(
                      maxExtent: _countWidth,
                      sliver: SliverToBoxAdapter(
                        child: SizedBox(
                          height: _chipHeight,
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: Text(
                              '${_length(group.first.first)}',
                              style: countStyle,
                            ),
                          ),
                        ),
                      ),
                    ),
                    SliverMainAxisGroup(
                      slivers: [
                        for (final (runIndex, run) in group.indexed)
                          SliverPadding(
                            padding: EdgeInsets.only(
                              bottom: runIndex == group.length - 1 ? 0 : _gap,
                            ),
                            sliver: SliverGrid.builder(
                              gridDelegate:
                                  SliverGridDelegateWithFixedCrossAxisCount(
                                    crossAxisCount: _columns(
                                      width,
                                      _width(run.first),
                                    ),
                                    mainAxisExtent: _chipHeight,
                                    crossAxisSpacing: _gap,
                                    mainAxisSpacing: _gap,
                                  ),
                              itemCount: run.length,
                              itemBuilder: (context, index) =>
                                  _WordChip(summary: run[index]),
                            ),
                          ),
                      ],
                    ),
                  ],
                ),
              ),
              // A line between 2 groups.
              if (index < groups.length - 1)
                SliverPadding(
                  padding: EdgeInsets.symmetric(horizontal: pagePadding.left),
                  sliver: SliverToBoxAdapter(
                    child: Divider(height: 24, color: colors.outlineVariant),
                  ),
                ),
            ],
          ],
        );
      },
    );
  }

  /// The length of a word, without its kana, as jpdag orders the words.
  static int _length(WordSummary summary) =>
      _label(summary).runes.where((rune) => !_isKana(rune)).length;

  /// The number of characters of a word, kana too, which gives the width of its chip.
  static int _width(WordSummary summary) => _label(summary).runes.length;

  /// Whether a character is kana: hiragana, katakana, or ー, as `jpdag::kana::is_kana` says.
  static bool _isKana(int rune) =>
      (rune >= 0x3041 && rune <= 0x3096) ||
      rune == 0x309D ||
      rune == 0x309E ||
      (rune >= 0x30A1 && rune <= 0x30FA) ||
      (rune >= 0x30FC && rune <= 0x30FE);

  /// The number of columns for words of a length in characters: as many as fit, with each chip as
  /// wide as the word.
  static int _columns(double width, int length) {
    final chipWidth = length * _fontSize + 2 * _chipPadding;
    return ((width + _gap) / (chipWidth + _gap)).floor().clamp(1, 1 << 16);
  }

  /// The written form that contains the kanji. It is not the headword of each word: the headword
  /// of 一寸 is ちょっと.
  static String _label(WordSummary summary) =>
      summary.matchedForm ?? summary.headword;
}

/// A word as a chip: the written form alone. The tooltip has the headword, the reading, and the
/// gloss. A tap opens the page of the word.
class _WordChip extends StatelessWidget {
  const _WordChip({required this.summary});

  final WordSummary summary;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Tooltip(
      positionDelegate: tooltipAboveRight,
      richMessage: wordTooltip(context, summary, withRank: true),
      waitDuration: const Duration(milliseconds: 300),
      child: Material(
        color: colors.surfaceContainerLow,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(6),
          side: BorderSide(color: colors.outlineVariant),
        ),
        clipBehavior: Clip.antiAlias,
        child: InkWell(
          onTap: () => openWordPage(Navigator.of(context), summary.id),
          child: Center(
            child: Text(
              _WordChips._label(summary),
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.fade,
              style: const TextStyle(
                fontSize: _WordChips._fontSize,
                height: 1.2,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
