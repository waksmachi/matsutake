import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// The space between a tooltip and its source.
const double _gap = 4;

/// Places a tooltip up and to the right of its source: the bottom left corner of the tooltip is
/// just past the top right corner of the source. A tooltip below its source would cover the text
/// under it, and the pointer that moves down the page would enter the tooltip. Up and to the right,
/// the pointer can leave the source to the left, to the right, up, or down, and reach the other
/// sources around it.
///
/// A tooltip with no room above its source goes below it. A tooltip with no room at the right
/// moves to the left as far as it must.
Offset tooltipAboveRight(TooltipPositionContext context) {
  final size = context.tooltipSize;
  final overlay = context.overlaySize;
  final halfWidth = context.targetSize.width / 2;
  final halfHeight = context.targetSize.height / 2;
  var top = context.target.dy - halfHeight - _gap - size.height;
  if (top < 0) {
    top = context.target.dy + halfHeight + _gap;
  }
  final left = (context.target.dx + halfWidth + _gap)
      .clamp(0.0, math.max(0.0, overlay.width - size.width))
      .toDouble();
  return Offset(left, top);
}
