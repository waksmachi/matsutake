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
        // The written forms with their readings, a line, then the senses. The page has no
        // headword: the forms show it.
        builder: (context, word) => SingleChildScrollView(
          padding: pagePadding,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              FormTable(word: word, fontSize: formFontSize),
              const Divider(height: 32),
              SenseList(senses: word.senses),
            ],
          ),
        ),
      ),
    );
  }
}

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

/// A word entry in few lines, for the kanji page: its written forms with their readings, then
/// its senses.
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
  });

  final Word word;
  final double fontSize;

  /// The kanji of the page that shows the table. Its link does not open the page again.
  final String? pageCharacter;

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

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final reading in word.readings) ...[
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
  }
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
