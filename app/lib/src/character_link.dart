import 'package:flutter/material.dart';

import 'kanji_page.dart';
import 'rust/api.dart';
import 'tooltip.dart';

/// A character that opens its kanji page. It looks like the text around it, with no chip and no
/// underline, and turns the vermilion of the theme (`primary`) under the pointer. A mutant has no
/// page, so it opens the page of its base, and its tooltip says "form of 人".
///
/// A character whose page is the current page does not open it again. Under the pointer it turns
/// a muted colour (`outline`), and the pointer stays an arrow.
class CharacterLinkText extends StatefulWidget {
  const CharacterLinkText({
    super.key,
    required this.link,
    required this.style,
    this.isCurrent = false,
  });

  final CharacterLink link;
  final TextStyle style;

  /// Whether the page of the character is the current page.
  final bool isCurrent;

  @override
  State<CharacterLinkText> createState() => _CharacterLinkTextState();
}

class _CharacterLinkTextState extends State<CharacterLinkText> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final link = widget.link;
    final base = link.base;
    final colors = Theme.of(context).colorScheme;
    final current = widget.isCurrent;
    Widget text = MouseRegion(
      cursor: current ? SystemMouseCursors.basic : SystemMouseCursors.click,
      onEnter: (event) => setState(() => _hovered = true),
      onExit: (event) => setState(() => _hovered = false),
      child: GestureDetector(
        onTap: current
            ? null
            : () =>
                  openKanjiPage(Navigator.of(context), base ?? link.character),
        child: Text(
          link.character,
          style: _hovered
              ? widget.style.copyWith(
                  color: current ? colors.outline : colors.primary,
                )
              : widget.style,
        ),
      ),
    );
    if (base != null) {
      text = Tooltip(
        message: 'form of $base',
        positionDelegate: tooltipAboveRight,
        child: text,
      );
    }
    return text;
  }
}

/// A text in which each character with a link opens its kanji page, as a [CharacterLinkText].
class LinkedText extends StatelessWidget {
  const LinkedText({
    super.key,
    required this.text,
    required this.links,
    required this.style,
    this.currentCharacter,
  });

  final String text;

  /// The link of each character that has one.
  final Map<String, CharacterLink> links;
  final TextStyle style;

  /// The character of the current page, if it is a kanji page. A link to it does not open it.
  final String? currentCharacter;

  @override
  Widget build(BuildContext context) {
    // The characters with no link stay together in 1 text.
    final children = <Widget>[];
    final plain = StringBuffer();
    void flush() {
      if (plain.isNotEmpty) {
        children.add(Text(plain.toString(), style: style));
        plain.clear();
      }
    }

    for (final character in text.characters) {
      final link = links[character];
      if (link == null) {
        plain.write(character);
      } else {
        flush();
        children.add(
          CharacterLinkText(
            link: link,
            style: style,
            isCurrent: (link.base ?? link.character) == currentCharacter,
          ),
        );
      }
    }
    flush();
    return Row(mainAxisSize: MainAxisSize.min, children: children);
  }
}
