import 'dart:math' as math;
import 'package:flutter/widgets.dart';

// 線ひと筋 秋の図案を 描きけり
enum LocoIcon {
  check,
  chevronDown,
  externalLink,
  headphones,
  laptop,
  mic,
  monitor,
  plus,
  radio,
  refreshCw,
  settings,
  smartphone,
  squareTerminal,
  triangleAlert,
  users,
  volume2,
}

class _Shape {
  const _Shape(this.path, {this.circle, this.rect});

  final String? path;
  final List<double>? circle;
  final List<double>? rect;
}

const Map<LocoIcon, List<_Shape>> _shapes = {
  LocoIcon.check: [
    _Shape('M20 6 9 17l-5-5'),
  ],
  LocoIcon.chevronDown: [
    _Shape('m6 9 6 6 6-6'),
  ],
  LocoIcon.externalLink: [
    _Shape('M15 3h6v6'),
    _Shape('M10 14 21 3'),
    _Shape('M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6'),
  ],
  LocoIcon.headphones: [
    _Shape(
      'M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3',
    ),
  ],
  LocoIcon.laptop: [
    _Shape(
      'M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z',
    ),
    _Shape('M20.054 15.987H3.946'),
  ],
  LocoIcon.mic: [
    _Shape('M12 19v3'),
    _Shape('M19 10v2a7 7 0 0 1-14 0v-2'),
    _Shape('', rect: [9, 2, 6, 13, 3]),
  ],
  LocoIcon.monitor: [
    _Shape('', rect: [2, 3, 20, 14, 2]),
    _Shape('M8 21H16'),
    _Shape('M12 17V21'),
  ],
  LocoIcon.plus: [
    _Shape('M5 12h14'),
    _Shape('M12 5v14'),
  ],
  LocoIcon.radio: [
    _Shape('M16.247 7.761a6 6 0 0 1 0 8.478'),
    _Shape('M19.075 4.933a10 10 0 0 1 0 14.134'),
    _Shape('M4.925 19.067a10 10 0 0 1 0-14.134'),
    _Shape('M7.753 16.239a6 6 0 0 1 0-8.478'),
    _Shape('', circle: [12, 12, 2]),
  ],
  LocoIcon.refreshCw: [
    _Shape('M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8'),
    _Shape('M21 3v5h-5'),
    _Shape('M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16'),
    _Shape('M8 16H3v5'),
  ],
  LocoIcon.settings: [
    _Shape(
      'M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915',
    ),
    _Shape('', circle: [12, 12, 3]),
  ],
  LocoIcon.smartphone: [
    _Shape('', rect: [5, 2, 14, 20, 2]),
    _Shape('M12 18h.01'),
  ],
  LocoIcon.squareTerminal: [
    _Shape('m7 11 2-2-2-2'),
    _Shape('M11 13h4'),
    _Shape('', rect: [3, 3, 18, 18, 2]),
  ],
  LocoIcon.triangleAlert: [
    _Shape('m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3'),
    _Shape('M12 9v4'),
    _Shape('M12 17h.01'),
  ],
  LocoIcon.users: [
    _Shape('M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2'),
    _Shape('M16 3.128a4 4 0 0 1 0 7.744'),
    _Shape('M22 21v-2a4 4 0 0 0-3-3.87'),
    _Shape('', circle: [9, 7, 4]),
  ],
  LocoIcon.volume2: [
    _Shape(
      'M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z',
    ),
    _Shape('M16 9a5 5 0 0 1 0 6'),
    _Shape('M19.364 18.364a9 9 0 0 0 0-12.728'),
  ],
};

class LucideIcon extends StatelessWidget {
  const LucideIcon(
    this.icon, {
    super.key,
    this.size = 20,
    this.strokeWidth = 2,
    required this.color,
  });

  final LocoIcon icon;
  final double size;
  final double strokeWidth;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: CustomPaint(
        painter: _IconPainter(icon, strokeWidth, color),
        isComplex: false,
      ),
    );
  }
}

class _IconPainter extends CustomPainter {
  _IconPainter(this.icon, this.strokeWidth, this.color);

  static final Map<LocoIcon, Path> _cache = {};

  final LocoIcon icon;
  final double strokeWidth;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final path = _cache.putIfAbsent(icon, () {
      final result = Path();
      for (final shape in _shapes[icon] ?? const <_Shape>[]) {
        if (shape.path != null && shape.path!.isNotEmpty) {
          _SvgPath.parse(shape.path!).addTo(result);
        }
        if (shape.circle != null) {
          final circle = shape.circle!;
          result.addOval(
            Rect.fromCircle(
              center: Offset(circle[0], circle[1]),
              radius: circle[2],
            ),
          );
        }
        if (shape.rect != null) {
          final rect = shape.rect!;
          result.addRRect(
            RRect.fromRectAndRadius(
              Rect.fromLTWH(rect[0], rect[1], rect[2], rect[3]),
              Radius.circular(rect[4]),
            ),
          );
        }
      }
      return result;
    });
    canvas.save();
    canvas.scale(size.width / 24, size.height / 24);
    final stroke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..color = color
      ..isAntiAlias = true;
    canvas.drawPath(path, stroke);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_IconPainter oldDelegate) =>
      oldDelegate.icon != icon ||
      oldDelegate.strokeWidth != strokeWidth ||
      oldDelegate.color != color;
}

// 一筆の 道筋辿る 秋の筆
class _SvgPath {
  _SvgPath._(this.data);

  final String data;
  final Path _path = Path();
  int _index = 0;
  double _currentX = 0;
  double _currentY = 0;
  double _startX = 0;
  double _startY = 0;
  double _lastControlX = 0;
  double _lastControlY = 0;
  String _lastCommand = '';

  static _SvgPath parse(String data) {
    final parser = _SvgPath._(data);
    parser._run();
    return parser;
  }

  void addTo(Path path) => path.addPath(_path, Offset.zero);

  void _run() {
    _skip();
    while (_index < data.length) {
      final command = data[_index];
      if (!_isCommand(command)) {
        _index++;
        continue;
      }
      _index++;
      switch (command) {
        case 'M':
        case 'm':
          _move(command == 'm');
        case 'L':
        case 'l':
          _line(command == 'l');
        case 'H':
        case 'h':
          _horizontal(command == 'h');
        case 'V':
        case 'v':
          _vertical(command == 'v');
        case 'C':
        case 'c':
          _cubic(command == 'c');
        case 'S':
        case 's':
          _smoothCubic(command == 's');
        case 'Q':
        case 'q':
          _quadratic(command == 'q');
        case 'T':
        case 't':
          _smoothQuadratic(command == 't');
        case 'A':
        case 'a':
          _arc(command == 'a');
        case 'Z':
        case 'z':
          _path.close();
          _currentX = _startX;
          _currentY = _startY;
        default:
          break;
      }
      _lastCommand = command;
      _skip();
    }
  }

  bool _isCommand(String value) => 'MmLlHhVvCcSsQqTtAaZz'.contains(value);

  void _skip() {
    while (_index < data.length) {
      final code = data.codeUnitAt(_index);
      if (code == 0x20 || code == 0x2C || code == 0x0A || code == 0x0D || code == 0x09) {
        _index++;
      } else {
        break;
      }
    }
  }

  double? _number() {
    _skip();
    final start = _index;
    if (_index < data.length && (data[_index] == '-' || data[_index] == '+')) _index++;
    while (_index < data.length && _isDigit(data.codeUnitAt(_index))) {
      _index++;
    }
    if (_index < data.length && data[_index] == '.') {
      _index++;
      while (_index < data.length && _isDigit(data.codeUnitAt(_index))) {
        _index++;
      }
    }
    if (_index < data.length && (data[_index] == 'e' || data[_index] == 'E')) {
      _index++;
      if (_index < data.length && (data[_index] == '-' || data[_index] == '+')) _index++;
      while (_index < data.length && _isDigit(data.codeUnitAt(_index))) {
        _index++;
      }
    }
    if (start == _index) return null;
    return double.tryParse(data.substring(start, _index));
  }

  bool _isDigit(int code) => code >= 0x30 && code <= 0x39;

  bool _hasNumber() {
    _skip();
    if (_index >= data.length) return false;
    final code = data.codeUnitAt(_index);
    return _isDigit(code) || code == 0x2D || code == 0x2B || code == 0x2E;
  }

  void _move(bool relative) {
    var first = true;
    while (_hasNumber()) {
      final x = _number();
      final y = _number();
      if (x == null || y == null) return;
      final targetX = relative ? _currentX + x : x;
      final targetY = relative ? _currentY + y : y;
      if (first) {
        _path.moveTo(targetX, targetY);
        _startX = targetX;
        _startY = targetY;
        first = false;
      } else {
        _path.lineTo(targetX, targetY);
      }
      _currentX = targetX;
      _currentY = targetY;
      _lastControlX = targetX;
      _lastControlY = targetY;
    }
  }

  void _line(bool relative) {
    while (_hasNumber()) {
      final x = _number();
      final y = _number();
      if (x == null || y == null) return;
      final targetX = relative ? _currentX + x : x;
      final targetY = relative ? _currentY + y : y;
      _path.lineTo(targetX, targetY);
      _currentX = targetX;
      _currentY = targetY;
      _lastControlX = targetX;
      _lastControlY = targetY;
    }
  }

  void _horizontal(bool relative) {
    while (_hasNumber()) {
      final x = _number();
      if (x == null) return;
      final targetX = relative ? _currentX + x : x;
      _path.lineTo(targetX, _currentY);
      _currentX = targetX;
      _lastControlX = targetX;
      _lastControlY = _currentY;
    }
  }

  void _vertical(bool relative) {
    while (_hasNumber()) {
      final y = _number();
      if (y == null) return;
      final targetY = relative ? _currentY + y : y;
      _path.lineTo(_currentX, targetY);
      _currentY = targetY;
      _lastControlX = _currentX;
      _lastControlY = targetY;
    }
  }

  void _cubic(bool relative) {
    while (_hasNumber()) {
      final c1x = _number();
      final c1y = _number();
      final c2x = _number();
      final c2y = _number();
      final x = _number();
      final y = _number();
      if ([c1x, c1y, c2x, c2y, x, y].contains(null)) return;
      final dc1x = relative ? _currentX + c1x! : c1x!;
      final dc1y = relative ? _currentY + c1y! : c1y!;
      final dc2x = relative ? _currentX + c2x! : c2x!;
      final dc2y = relative ? _currentY + c2y! : c2y!;
      final dx = relative ? _currentX + x! : x!;
      final dy = relative ? _currentY + y! : y!;
      _path.cubicTo(dc1x, dc1y, dc2x, dc2y, dx, dy);
      _lastControlX = dc2x;
      _lastControlY = dc2y;
      _currentX = dx;
      _currentY = dy;
    }
  }

  void _smoothCubic(bool relative) {
    while (_hasNumber()) {
      final c2x = _number();
      final c2y = _number();
      final x = _number();
      final y = _number();
      if ([c2x, c2y, x, y].contains(null)) return;
      final reflective = _lastCommand == 'C' || _lastCommand == 'c' ||
          _lastCommand == 'S' || _lastCommand == 's';
      final c1x = reflective ? 2 * _currentX - _lastControlX : _currentX;
      final c1y = reflective ? 2 * _currentY - _lastControlY : _currentY;
      final dc2x = relative ? _currentX + c2x! : c2x!;
      final dc2y = relative ? _currentY + c2y! : c2y!;
      final dx = relative ? _currentX + x! : x!;
      final dy = relative ? _currentY + y! : y!;
      _path.cubicTo(c1x, c1y, dc2x, dc2y, dx, dy);
      _lastControlX = dc2x;
      _lastControlY = dc2y;
      _currentX = dx;
      _currentY = dy;
    }
  }

  void _quadratic(bool relative) {
    while (_hasNumber()) {
      final c1x = _number();
      final c1y = _number();
      final x = _number();
      final y = _number();
      if ([c1x, c1y, x, y].contains(null)) return;
      final dc1x = relative ? _currentX + c1x! : c1x!;
      final dc1y = relative ? _currentY + c1y! : c1y!;
      final dx = relative ? _currentX + x! : x!;
      final dy = relative ? _currentY + y! : y!;
      _path.quadraticBezierTo(dc1x, dc1y, dx, dy);
      _lastControlX = dc1x;
      _lastControlY = dc1y;
      _currentX = dx;
      _currentY = dy;
    }
  }

  void _smoothQuadratic(bool relative) {
    while (_hasNumber()) {
      final x = _number();
      final y = _number();
      if (x == null || y == null) return;
      final reflective = _lastCommand == 'Q' || _lastCommand == 'q' ||
          _lastCommand == 'T' || _lastCommand == 't';
      final c1x = reflective ? 2 * _currentX - _lastControlX : _currentX;
      final c1y = reflective ? 2 * _currentY - _lastControlY : _currentY;
      final dx = relative ? _currentX + x : x;
      final dy = relative ? _currentY + y : y;
      _path.quadraticBezierTo(c1x, c1y, dx, dy);
      _lastControlX = c1x;
      _lastControlY = c1y;
      _currentX = dx;
      _currentY = dy;
    }
  }

  void _arc(bool relative) {
    while (_hasNumber()) {
      final rx = _number();
      final ry = _number();
      final rotation = _number();
      final largeArc = _number();
      final sweep = _number();
      final x = _number();
      final y = _number();
      if ([rx, ry, rotation, largeArc, sweep, x, y].contains(null)) return;
      final targetX = relative ? _currentX + x! : x!;
      final targetY = relative ? _currentY + y! : y!;
      _arcTo(
        _currentX,
        _currentY,
        targetX,
        targetY,
        rx!.abs(),
        ry!.abs(),
        rotation! * math.pi / 180,
        largeArc != 0,
        sweep != 0,
      );
      _currentX = targetX;
      _currentY = targetY;
      _lastControlX = targetX;
      _lastControlY = targetY;
    }
  }

  void _arcTo(
    double x1,
    double y1,
    double x2,
    double y2,
    double rx,
    double ry,
    double angle,
    bool largeArc,
    bool sweep,
  ) {
    if (rx == 0 || ry == 0) {
      _path.lineTo(x2, y2);
      return;
    }
    final cos = math.cos(angle);
    final sin = math.sin(angle);
    final dx = (x1 - x2) / 2;
    final dy = (y1 - y2) / 2;
    final x1p = cos * dx + sin * dy;
    final y1p = -sin * dx + cos * dy;
    var scale = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if (scale > 1) {
      final root = math.sqrt(scale);
      rx *= root;
      ry *= root;
      scale = 1;
    }
    final sign = largeArc == sweep ? -1.0 : 1.0;
    final numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    final denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    final factor = sign * math.sqrt(math.max(0, numerator / denominator));
    final cxp = factor * rx * y1p / ry;
    final cyp = -factor * ry * x1p / rx;
    final cx = cos * cxp - sin * cyp + (x1 + x2) / 2;
    final cy = sin * cxp + cos * cyp + (y1 + y2) / 2;
    final startAngle = math.atan2((y1p - cyp) / ry, (x1p - cxp) / rx);
    final endAngle = math.atan2((-y1p - cyp) / ry, (-x1p - cxp) / rx);
    var delta = endAngle - startAngle;
    if (!sweep && delta > 0) delta -= 2 * math.pi;
    if (sweep && delta < 0) delta += 2 * math.pi;
    final segments = (delta.abs() / (math.pi / 2)).ceil().clamp(1, 8);
    final step = delta / segments;
    final k = 4 / 3 * math.tan(step / 4);
    var angleStart = startAngle;
    var pointX = math.cos(angleStart);
    var pointY = math.sin(angleStart);
    for (var index = 0; index < segments; index++) {
      final angleEnd = angleStart + step;
      final nextX = math.cos(angleEnd);
      final nextY = math.sin(angleEnd);
      final control1X = pointX - k * pointY;
      final control1Y = pointY + k * pointX;
      final control2X = nextX + k * nextY;
      final control2Y = nextY - k * nextX;
      _path.cubicTo(
        cx + rx * (cos * control1X - sin * control1Y),
        cy + ry * (sin * control1X + cos * control1Y),
        cx + rx * (cos * control2X - sin * control2Y),
        cy + ry * (sin * control2X + cos * control2Y),
        cx + rx * (cos * nextX - sin * nextY),
        cy + ry * (sin * nextX + cos * nextY),
      );
      angleStart = angleEnd;
      pointX = nextX;
      pointY = nextY;
    }
  }
}
