import 'package:flutter/material.dart';

// 墨と紙 秋の配色 そのままに
class LocoColors {
  const LocoColors({
    required this.brightness,
    required this.bg,
    required this.surface,
    required this.hover,
    required this.line,
    required this.lineStrong,
    required this.ink,
    required this.muted,
    required this.accent,
    required this.inverse,
    required this.onInverse,
    required this.knob,
    required this.markBg,
    required this.markInk,
  });

  final Brightness brightness;
  final Color bg;
  final Color surface;
  final Color hover;
  final Color line;
  final Color lineStrong;
  final Color ink;
  final Color muted;
  final Color accent;
  final Color inverse;
  final Color onInverse;
  final Color knob;
  final Color markBg;
  final Color markInk;

  bool get isDark => brightness == Brightness.dark;

  static const LocoColors light = LocoColors(
    brightness: Brightness.light,
    bg: Color(0xFFF7F7F6),
    surface: Color(0xFFFFFFFF),
    hover: Color(0xFFEDEDEB),
    line: Color(0xFFE3E3E0),
    lineStrong: Color(0xFFCBCBC7),
    ink: Color(0xFF1A1A19),
    muted: Color(0xFF6E6E6A),
    accent: Color(0xFFD6361F),
    inverse: Color(0xFF262625),
    onInverse: Color(0xFFF4F4F2),
    knob: Color(0xFFFFFFFF),
    markBg: Color(0xFF1A1A19),
    markInk: Color(0xFFF7F7F6),
  );

  static const LocoColors dark = LocoColors(
    brightness: Brightness.dark,
    bg: Color(0xFF161615),
    surface: Color(0xFF1F1F1E),
    hover: Color(0xFF262625),
    line: Color(0xFF2C2C2A),
    lineStrong: Color(0xFF444441),
    ink: Color(0xFFECECEA),
    muted: Color(0xFF9A9A95),
    accent: Color(0xFFEF5A43),
    inverse: Color(0xFFECECEA),
    onInverse: Color(0xFF161615),
    knob: Color(0xFFECECEA),
    markBg: Color(0xFFECECEA),
    markInk: Color(0xFF161615),
  );

  static LocoColors of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<LocoScope>();
    return scope?.colors ??
        (MediaQuery.platformBrightnessOf(context) == Brightness.dark ? dark : light);
  }
}

class LocoScope extends InheritedWidget {
  const LocoScope({super.key, required this.colors, required super.child});

  final LocoColors colors;

  @override
  bool updateShouldNotify(LocoScope oldWidget) => oldWidget.colors != colors;
}

// 十五の 文字と行間 そのままで
class LocoText {
  const LocoText._();

  static const String family = 'Selawik';
  static const List<String> fallback = [
    'Noto Sans CJK SC',
    'Noto Sans SC',
    'Microsoft YaHei UI',
    'sans-serif',
  ];

  static TextStyle base(LocoColors colors) => TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: 15,
        height: 1.45,
        color: colors.ink,
      );

  static TextStyle h1(LocoColors colors) => TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: 24,
        height: 1.2,
        fontWeight: FontWeight.w600,
        letterSpacing: -0.24,
        color: colors.ink,
      );

  static TextStyle h2(LocoColors colors) => TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: 19,
        height: 1.25,
        fontWeight: FontWeight.w600,
        letterSpacing: -0.19,
        color: colors.ink,
      );

  static TextStyle meta(LocoColors colors) => TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: 12,
        height: 1.45,
        color: colors.muted,
      );

  static TextStyle body(
    LocoColors colors, {
    double size = 15,
    FontWeight weight = FontWeight.w400,
    Color? color,
    double height = 1.45,
    double? letterSpacing,
    bool tabular = false,
  }) =>
      TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: size,
        height: height,
        fontWeight: weight,
        color: color ?? colors.ink,
        letterSpacing: letterSpacing,
        fontFeatures: tabular ? const [FontFeature.tabularFigures()] : null,
      );

  static TextStyle strong(LocoColors colors, {Color? color}) => TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: 15,
        height: 1.45,
        fontWeight: FontWeight.w600,
        color: color ?? colors.ink,
      );

  static TextStyle small(LocoColors colors, {Color? color, FontWeight? weight}) =>
      TextStyle(
        fontFamily: family,
        fontFamilyFallback: fallback,
        fontSize: 12,
        height: 1.45,
        fontWeight: weight ?? FontWeight.w400,
        color: color ?? colors.muted,
        fontFeatures: const [FontFeature.tabularFigures()],
      );
}

// 四つの角 朱の線引き 秋の窓
class LocoBox {
  const LocoBox._();

  static const double radius = 4;
}
