import 'package:flutter/material.dart';

import 'kana_ink.dart';

/// A reading with its pitch accent: a line over the morae of high pitch, and a tick down where the
/// pitch falls. In Tokyo Japanese the first mora is low unless the accent falls after it, and the
/// morae after the fall are low.
///
/// - はし as 箸 (downstep 1): a line over は, and a tick after it.
/// - はし as 橋 (downstep 2): a line over し, and a tick after it. The pitch falls on the particle.
/// - はし as 端 (downstep 0): a line over し that runs on past the end with no tick, because the
///   pitch stays high on the particle. So a reading with no fall looks different from a reading
///   with no accent.
class PitchText extends StatelessWidget {
  const PitchText({
    super.key,
    required this.morae,
    required this.downstep,
    required this.style,
    this.lineColor,
  });

  final List<String> morae;

  /// Number of morae before the fall in pitch, or 0 for no fall.
  final int downstep;

  final TextStyle style;

  /// The colour of the line and the tick. By default, the colour of the text.
  final Color? lineColor;

  /// The height of the line above the baseline, as a fraction of the font size. The line height
  /// of the text leaves space above the kana, and the line sits in it, just above the kana.
  static const _rise = 0.88;

  /// Space above the text, in logical pixels, which moves the text down from the line.
  static const _padding = 2.0;

  /// The length of the line past the end of a reading with no fall, and of a tick, as fractions
  /// of the font size.
  static const _runOn = 0.4;
  static const _tick = 0.3;

  /// The space between the ink of the last kana and a fall at the end of the reading, as a
  /// fraction of the font size.
  static const _endGap = 0.1;

  @override
  Widget build(BuildContext context) {
    final effective = DefaultTextStyle.of(context).style.merge(style);
    final fontSize = MediaQuery.textScalerOf(context)
        .scale(effective.fontSize ?? 14);
    final text = morae.join();
    return CustomPaint(
      foregroundPainter: _PitchPainter(
        morae: morae,
        downstep: downstep,
        style: effective,
        color: lineColor ?? effective.color ?? const Color(0xFF000000),
        textScaler: MediaQuery.textScalerOf(context),
        rise: fontSize * _rise,
        runOn: fontSize * _runOn,
        tick: fontSize * _tick,
        fontSize: fontSize,
        endGap: fontSize * _endGap,
      ),
      child: Padding(
        padding: EdgeInsets.only(
          top: _padding,
          right: downstep == 0 ? fontSize * _runOn : 0,
        ),
        child: Text(text, style: style),
      ),
    );
  }
}

class _PitchPainter extends CustomPainter {
  _PitchPainter({
    required this.morae,
    required this.downstep,
    required this.style,
    required this.color,
    required this.textScaler,
    required this.rise,
    required this.runOn,
    required this.tick,
    required this.fontSize,
    required this.endGap,
  });

  final List<String> morae;
  final int downstep;
  final TextStyle style;
  final Color color;
  final TextScaler textScaler;

  /// The height of the line above the baseline, and the lengths of the run-on line and the tick.
  final double rise;
  final double runOn;
  final double tick;

  /// The font size after the text scaler.
  final double fontSize;
  final double endGap;

  /// Whether the mora at `index` is high.
  bool _high(int index) => switch (downstep) {
    0 => index > 0,
    1 => index == 0,
    _ => index > 0 && index < downstep,
  };

  /// The edges of the ink of a kana, as fractions of the font size from the start of its box. A
  /// character that is not in the table has ink across its whole box.
  static (double, double) _ink(String character) =>
      kanaInk[character.codeUnitAt(0)] ?? (0, 1);

  @override
  void paint(Canvas canvas, Size size) {
    final text = TextPainter(
      text: TextSpan(text: morae.join(), style: style),
      textDirection: TextDirection.ltr,
      textScaler: textScaler,
    )..layout();
    double x(int offset) =>
        text.getOffsetForCaret(TextPosition(offset: offset), Rect.zero).dx;
    // The x of the start of each mora, and of the end of the reading.
    final edges = <double>[];
    final starts = <int>[];
    var offset = 0;
    for (final mora in morae) {
      starts.add(offset);
      edges.add(x(offset));
      offset += mora.length;
    }
    starts.add(offset);
    edges.add(text.width);
    // The x of a fall in pitch before the mora at `index`: halfway between the ink of the last
    // kana before it and the ink of the first kana after it. A fall at the end of the reading is
    // a little after the ink of the last kana.
    double fall(int index) {
      final last = starts[index] - 1;
      final inkEnd =
          x(last) + _ink(morae.join().substring(last, last + 1)).$2 * fontSize;
      if (index == morae.length) return inkEnd + endGap;
      final inkStart = edges[index] + _ink(morae[index]).$1 * fontSize;
      return (inkEnd + inkStart) / 2;
    }

    final top = text.computeLineMetrics().first.baseline - rise;

    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.2
      ..strokeCap = StrokeCap.square;
    // The high morae are 1 run: the first mora alone, or the morae from the second.
    final high = [
      for (var i = 0; i < morae.length; i++)
        if (_high(i)) i,
    ];
    final end = downstep == 0 ? edges.last + runOn : null;
    if (high.isNotEmpty || end != null) {
      final start = high.isEmpty ? edges.last : edges[high.first];
      final stop = end ?? fall(high.last + 1);
      canvas.drawLine(Offset(start, top), Offset(stop, top), paint);
    }
    // Ingest skips an accent after the last mora.
    if (downstep > 0 && downstep < edges.length) {
      final at = fall(downstep);
      canvas.drawLine(Offset(at, top), Offset(at, top + tick), paint);
    }
    text.dispose();
  }

  @override
  bool shouldRepaint(_PitchPainter old) =>
      old.downstep != downstep ||
      old.style != style ||
      old.color != color ||
      old.textScaler != textScaler ||
      old.morae.join() != morae.join();
}
