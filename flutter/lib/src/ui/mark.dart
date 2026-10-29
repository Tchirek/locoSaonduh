import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../theme.dart';

// 北斗めく 折れ道の川 明り行く
class LocoMark extends StatefulWidget {
  const LocoMark({super.key, this.size = 32, this.writhing = false});

  final double size;
  final bool writhing;

  @override
  State<LocoMark> createState() => _LocoMarkState();
}

class _LocoMarkState extends State<LocoMark> with SingleTickerProviderStateMixin {
  static const String _wave =
      'M220 540 H268 C320 540 330 420 395 420 C460 420 452 636 525 636 '
      'C598 636 610 442 672 442 C724 442 742 512 804 540';

  static const double _length = 1000;
  static const double _head = 230;
  static const double _tail = 170;
  static const double _travel = 1.35;
  static const double _cycle = 1.9;
  static const double _fillSpan = 0.5;
  static const double _dimSpan = 0.35;
  static const double _dim = 0.26;
  static const double _dot = 58;
  static const double _hole = 22;
  static const double _reach = _length + _head + _tail;
  static final double _arrive =
      _travel * math.acos(1 - (2 * _length * 0.95) / _reach) / math.pi;

  late final Ticker _ticker;
  double _clock = 0;
  double _dimValue = 0;
  bool _cycling = false;
  Duration _last = Duration.zero;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_tick);
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(covariant LocoMark oldWidget) {
    super.didUpdateWidget(oldWidget);
    _sync();
  }

  void _sync() {
    final reduced = MediaQuery.of(context).disableAnimations;
    final wanted = widget.writhing && !reduced;
    if (wanted && !_ticker.isActive) {
      _last = Duration.zero;
      _ticker.start();
    }
    if (!wanted && !_cycling && _dimValue <= 0) {
      if (_ticker.isActive) _ticker.stop();
    }
  }

  void _tick(Duration elapsed) {
    final delta = _last == Duration.zero
        ? 0.0
        : math.min(0.05, (elapsed - _last).inMicroseconds / 1e6);
    _last = elapsed;
    final reduced = MediaQuery.of(context).disableAnimations;
    final wanted = widget.writhing && !reduced;
    if (wanted && !_cycling) {
      _cycling = true;
      _clock = 0;
    }
    if (_cycling) {
      _clock += delta;
      if (_clock >= _cycle) {
        _clock -= _cycle;
        _cycling = wanted;
      }
    }
    // 流れ終へ 墨色戻る 秋の川
    final step = delta / _dimSpan;
    _dimValue = _cycling
        ? math.min(1, _dimValue + step)
        : math.max(0, _dimValue - step);
    if (!_cycling && _dimValue == 0) {
      _ticker.stop();
      setState(() {});
      return;
    }
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return SizedBox(
      width: widget.size,
      height: widget.size,
      child: CustomPaint(
        painter: _MarkPainter(
          colors: colors,
          wave: _wave,
          clock: _clock,
          cycling: _cycling,
          dim: _cycling ? _dimValue : _dimValue,
          head: _head,
          tail: _tail,
          length: _length,
          travel: _travel,
          fillSpan: _fillSpan,
          arrive: _arrive,
          dimTarget: _dim,
          dot: _dot,
          hole: _hole,
        ),
      ),
    );
  }
}

class _MarkPainter extends CustomPainter {
  _MarkPainter({
    required this.colors,
    required this.wave,
    required this.clock,
    required this.cycling,
    required this.dim,
    required this.head,
    required this.tail,
    required this.length,
    required this.travel,
    required this.fillSpan,
    required this.arrive,
    required this.dimTarget,
    required this.dot,
    required this.hole,
  });

  final LocoColors colors;
  final String wave;
  final double clock;
  final bool cycling;
  final double dim;
  final double head;
  final double tail;
  final double length;
  final double travel;
  final double fillSpan;
  final double arrive;
  final double dimTarget;
  final double dot;
  final double hole;

  static Path? _cachedPath;

  double _ease(double value) =>
      (1 - math.cos(math.pi * value.clamp(0.0, 1.0))) / 2;

  Path _path() {
    if (_cachedPath != null) return _cachedPath!;
    final path = Path()
      ..moveTo(220, 540)
      ..lineTo(268, 540)
      ..cubicTo(320, 540, 330, 420, 395, 420)
      ..cubicTo(460, 420, 452, 636, 525, 636)
      ..cubicTo(598, 636, 610, 442, 672, 442)
      ..cubicTo(724, 442, 742, 512, 804, 540);
    _cachedPath = path;
    return path;
  }

  @override
  void paint(Canvas canvas, Size size) {
    final scale = size.width / 1024;
    canvas.save();
    canvas.scale(scale, scale);

    final background = Paint()..color = colors.markBg;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        const Rect.fromLTWH(0, 0, 1024, 1024),
        const Radius.circular(232),
      ),
      background,
    );

    final path = _path();
    final metric = path.computeMetrics().first;
    final unit = metric.length / length;

    final stroke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 64
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..color = colors.markInk
      ..isAntiAlias = true;

    final track = 1 - (1 - dimTarget) * _ease(dim);
    stroke.color = colors.markInk.withValues(alpha: colors.markInk.a * track);
    canvas.drawPath(path, stroke);

    final travelHead = cycling ? _ease(clock / travel) * (length + head + tail) : 0.0;
    final bright = _span(travelHead - head, travelHead, unit);
    final faint = _span(travelHead - head - tail, travelHead - head, unit);
    if (faint != null) {
      stroke.color = colors.markInk.withValues(alpha: colors.markInk.a * 0.55 * track);
      canvas.drawPath(metric.extractPath(faint.$1, faint.$2), stroke);
    }
    if (bright != null) {
      stroke.color = colors.markInk.withValues(alpha: colors.markInk.a * track);
      canvas.drawPath(metric.extractPath(bright.$1, bright.$2), stroke);
    }

    // 辿り着き 朱き輪ひとつ 満ちにけり
    final since = clock - arrive;
    final fill = cycling && since > 0 && since < fillSpan
        ? math.sin(math.pi * since / fillSpan)
        : 0.0;
    final dotRadius = dot * (1 + 0.06 * fill);
    final holeRadius = hole * (1 - fill);
    canvas.drawCircle(
      const Offset(812, 540),
      dotRadius,
      Paint()..color = colors.accent,
    );
    canvas.drawCircle(
      const Offset(812, 540),
      holeRadius,
      Paint()..color = colors.markBg,
    );
    canvas.restore();
  }

  (double, double)? _span(double from, double to, double unit) {
    final start = math.max(0, from);
    final end = math.min(length, to);
    if (end - start < 0.5) return null;
    return (start * unit, end * unit);
  }

  @override
  bool shouldRepaint(_MarkPainter oldDelegate) =>
      oldDelegate.clock != clock ||
      oldDelegate.cycling != cycling ||
      oldDelegate.dim != dim ||
      oldDelegate.colors != colors;
}
