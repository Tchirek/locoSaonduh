import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../icons.dart';
import '../theme.dart';

// 四角なる 器に声らを 盛りつけ
class LocoPage extends StatelessWidget {
  const LocoPage({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    final compact = width < 560;
    return Align(
      alignment: Alignment.topCenter,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 640),
        child: Padding(
          padding: compact
              ? const EdgeInsets.fromLTRB(16, 16, 16, 24)
              : const EdgeInsets.fromLTRB(24, 24, 24, 36),
          child: child,
        ),
      ),
    );
  }
}

class LocoSection extends StatelessWidget {
  const LocoSection({super.key, required this.child, this.top = false});

  final Widget child;
  final bool top;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(top: top ? 32 : 0),
      child: child,
    );
  }
}

// 見出しに 数を添えて 秋の空
class SectionHead extends StatelessWidget {
  const SectionHead({
    super.key,
    required this.title,
    this.count,
    this.tools,
    this.note,
  });

  final String title;
  final int? count;
  final Widget? tools;
  final Widget? note;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 32),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.baseline,
          textBaseline: TextBaseline.alphabetic,
          children: [
            Text(title, style: LocoText.h2(colors)),
            if (count != null) ...[
              const SizedBox(width: 8),
              Text(
                '$count',
                style: LocoText.meta(colors).copyWith(
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ],
            if (note != null) ...[
              const SizedBox(width: 8),
              Flexible(child: note!),
            ],
            const Spacer(),
            if (tools != null)
              Padding(
                padding: const EdgeInsets.only(left: 2),
                child: Align(alignment: Alignment.center, child: tools),
              ),
          ],
        ),
      ),
    );
  }
}

class LocoList extends StatelessWidget {
  const LocoList({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: colors.line)),
      ),
      child: Column(mainAxisSize: MainAxisSize.min, children: children),
    );
  }
}

// 札一枚 並べて人の 名を読まむ
class LocoRow extends StatelessWidget {
  const LocoRow({
    super.key,
    required this.children,
    this.minHeight = 56,
    this.padding = const EdgeInsets.symmetric(vertical: 10),
    this.wrap = false,
  });

  final List<Widget> children;
  final double minHeight;
  final EdgeInsets padding;
  final bool wrap;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Container(
      constraints: BoxConstraints(minHeight: minHeight),
      padding: padding,
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: colors.line)),
      ),
      child: wrap
          ? Wrap(
              spacing: 12,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: children,
            )
          : Row(children: children),
    );
  }
}

class RowTitle extends StatelessWidget {
  const RowTitle(this.text, {super.key, this.flex = false});

  final String text;
  final bool flex;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final child = Text(
      text,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      softWrap: false,
      style: LocoText.strong(colors),
    );
    return flex ? Expanded(child: child) : Flexible(child: child);
  }
}

class MetaText extends StatelessWidget {
  const MetaText(this.text, {super.key, this.tabular = true, this.color});

  final String text;
  final bool tabular;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Text(
      text,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      softWrap: false,
      style: LocoText.meta(colors).copyWith(
        color: color,
        fontFeatures: tabular ? const [FontFeature.tabularFigures()] : null,
      ),
    );
  }
}

// 朱の点 今の声の 在り処なり
class LocoDot extends StatelessWidget {
  const LocoDot({super.key, this.idle = false});

  final bool idle;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Container(
      width: 6,
      height: 6,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: idle ? colors.muted : colors.accent,
      ),
    );
  }
}

class LocoIconButton extends StatefulWidget {
  const LocoIconButton({
    super.key,
    required this.icon,
    required this.label,
    this.onPressed,
    this.active = false,
    this.spinning = false,
    this.iconSize = 17,
    this.strokeWidth = 1.8,
  });

  final LocoIcon icon;
  final String label;
  final VoidCallback? onPressed;
  final bool active;
  final bool spinning;
  final double iconSize;
  final double strokeWidth;

  @override
  State<LocoIconButton> createState() => _LocoIconButtonState();
}

class _LocoIconButtonState extends State<LocoIconButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final enabled = widget.onPressed != null;
    final highlighted = enabled && (widget.active || _hover);
    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onPressed,
        behavior: HitTestBehavior.opaque,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 140),
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            color: highlighted ? colors.hover : Colors.transparent,
            borderRadius: BorderRadius.circular(LocoBox.radius),
          ),
          child: Opacity(
            opacity: enabled ? 1 : 0.45,
            child: Center(
              child: SpinningBuilder(
                spinning: widget.spinning,
                child: LucideIcon(
                  widget.icon,
                  size: widget.iconSize,
                  strokeWidth: widget.strokeWidth,
                  color: highlighted ? colors.ink : colors.muted,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class SpinningBuilder extends StatefulWidget {
  const SpinningBuilder({super.key, required this.spinning, required this.child});

  final bool spinning;
  final Widget child;

  @override
  State<SpinningBuilder> createState() => _SpinningBuilderState();
}

class _SpinningBuilderState extends State<SpinningBuilder>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker((elapsed) {
    setState(() {
      _angle = elapsed.inMicroseconds / 900000 * 2 * math.pi;
    });
  });
  double _angle = 0;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(covariant SpinningBuilder oldWidget) {
    super.didUpdateWidget(oldWidget);
    _sync();
  }

  void _sync() {
    final reduced = MediaQuery.of(context).disableAnimations;
    if (widget.spinning && !reduced && !_ticker.isActive) {
      _ticker.start();
    } else if ((!widget.spinning || reduced) && _ticker.isActive) {
      _ticker.stop();
      _angle = 0;
    }
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!widget.spinning) return widget.child;
    return Transform.rotate(angle: _angle, child: widget.child);
  }
}

// 押し心地 静かに灯る 秋の釦
class LocoButton extends StatefulWidget {
  const LocoButton(
    this.label, {
    super.key,
    this.onPressed,
    this.icon,
    this.iconSize = 14,
  });

  final String label;
  final VoidCallback? onPressed;
  final LocoIcon? icon;
  final double iconSize;

  @override
  State<LocoButton> createState() => _LocoButtonState();
}

class _LocoButtonState extends State<LocoButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final enabled = widget.onPressed != null;
    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onPressed,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 140),
          height: 32,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(LocoBox.radius),
            border: Border.all(
              color: _hover && enabled ? colors.ink : colors.lineStrong,
            ),
          ),
          child: Opacity(
            opacity: enabled ? 1 : 0.45,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                if (widget.icon != null) ...[
                  LucideIcon(widget.icon!, size: widget.iconSize, color: colors.ink),
                  const SizedBox(width: 6),
                ],
                Text(widget.label, style: LocoText.base(colors)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// 押し替へて 朱の道をば 選びけり
class LocoSwitch extends StatelessWidget {
  const LocoSwitch({
    super.key,
    required this.checked,
    required this.label,
    required this.onChanged,
    this.disabled = false,
    this.busy = false,
  });

  final bool checked;
  final String label;
  final ValueChanged<bool>? onChanged;
  final bool disabled;
  final bool busy;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final enabled = !disabled && !busy && onChanged != null;
    return Semantics(
      label: label,
      toggled: checked,
      child: GestureDetector(
        onTap: enabled ? () => onChanged!(!checked) : null,
        behavior: HitTestBehavior.opaque,
        child: MouseRegion(
          cursor: enabled
              ? SystemMouseCursors.click
              : busy
                  ? SystemMouseCursors.progress
                  : SystemMouseCursors.basic,
          child: SizedBox(
            width: 40,
            height: 24,
            child: Stack(
              children: [
                AnimatedContainer(
                  duration: const Duration(milliseconds: 160),
                  decoration: BoxDecoration(
                    color: checked ? colors.accent : colors.lineStrong,
                    borderRadius: BorderRadius.circular(12),
                  ),
                ),
                AnimatedPositioned(
                  duration: const Duration(milliseconds: 160),
                  curve: Curves.easeOut,
                  left: checked ? 19 : 3,
                  top: 3,
                  child: Container(
                    width: 18,
                    height: 18,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: colors.knob,
                      boxShadow: const [
                        BoxShadow(
                          color: Color(0x2E000000),
                          offset: Offset(0, 1),
                          blurRadius: 2,
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// 一つづつ 灯を点して 数を示す
class LevelMeter extends StatelessWidget {
  const LevelMeter({
    super.key,
    required this.level,
    this.bars = 28,
    this.tone = LevelTone.ink,
  });

  final double level;
  final int bars;
  final LevelTone tone;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final clamped = level.clamp(0.0, 1.0);
    final lit = (clamped * bars).round();
    final active = tone == LevelTone.accent ? colors.accent : colors.ink;
    return SizedBox(
      height: 16,
      child: Row(
        children: [
          for (var index = 0; index < bars; index++) ...[
            Expanded(
              child: AnimatedContainer(
                duration: const Duration(milliseconds: 120),
                height: 6,
                decoration: BoxDecoration(
                  color: index < lit ? active : colors.lineStrong,
                  borderRadius: BorderRadius.circular(1),
                ),
              ),
            ),
            if (index != bars - 1) const SizedBox(width: 2),
          ],
        ],
      ),
    );
  }
}

enum LevelTone { ink, accent }

class DeviceIcon extends StatelessWidget {
  const DeviceIcon({super.key, required this.platform});

  final String platform;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final icon = switch (platform) {
      'android' || 'ios' => LocoIcon.smartphone,
      'macos' || 'windows' => LocoIcon.monitor,
      'linux' => LocoIcon.squareTerminal,
      _ => LocoIcon.laptop,
    };
    return SizedBox(
      width: 24,
      child: Center(
        child: LucideIcon(icon, size: 20, strokeWidth: 1.6, color: colors.muted),
      ),
    );
  }
}

// 選び取る 棚の品々 指先に
class SelectBox extends StatefulWidget {
  const SelectBox({
    super.key,
    required this.value,
    required this.items,
    required this.onChanged,
    this.disabled = false,
    this.compact = false,
    this.label,
  });

  final String value;
  final List<SelectItem> items;
  final ValueChanged<String>? onChanged;
  final bool disabled;
  final bool compact;
  final String? label;

  @override
  State<SelectBox> createState() => _SelectBoxState();
}

class SelectItem {
  const SelectItem(this.value, this.label);

  final String value;
  final String label;
}

class _SelectBoxState extends State<SelectBox> {
  final _key = GlobalKey();
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final enabled = !widget.disabled && widget.onChanged != null;
    final current = widget.items.where((item) => item.value == widget.value);
    final text = current.isEmpty ? '' : current.first.label;
    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        key: _key,
        onTap: enabled ? _open : null,
        child: Container(
          height: widget.compact ? 30 : 32,
          padding: const EdgeInsets.only(left: 10, right: 30),
          decoration: BoxDecoration(
            color: widget.compact ? colors.bg : colors.surface,
            borderRadius: BorderRadius.circular(LocoBox.radius),
            border: Border.all(
              color: _hover && enabled ? colors.ink : colors.lineStrong,
            ),
          ),
          child: Stack(
            alignment: Alignment.centerLeft,
            children: [
              Text(
                text,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                softWrap: false,
                style: LocoText.body(
                  colors,
                  size: widget.compact ? 12 : 15,
                ).copyWith(
                  color: enabled ? colors.ink : colors.ink.withValues(alpha: 0.45),
                ),
              ),
              Positioned(
                right: -21,
                child: LucideIcon(
                  LocoIcon.chevronDown,
                  size: widget.compact ? 14 : 15,
                  strokeWidth: 1.7,
                  color: colors.muted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _open() async {
    final render = _key.currentContext?.findRenderObject() as RenderBox?;
    if (render == null) return;
    final origin = render.localToGlobal(Offset.zero);
    final overlay = Overlay.of(context).context.findRenderObject() as RenderBox?;
    final colors = LocoColors.of(context);
    final selected = await showMenu<String>(
      context: context,
      color: colors.surface,
      surfaceTintColor: Colors.transparent,
      elevation: 2,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(LocoBox.radius),
        side: BorderSide(color: colors.lineStrong),
      ),
      position: RelativeRect.fromRect(
        Rect.fromLTWH(origin.dx, origin.dy + render.size.height, render.size.width, 0),
        Offset.zero & (overlay?.size ?? render.size),
      ),
      items: [
        for (final item in widget.items)
          PopupMenuItem<String>(
            value: item.value,
            height: 34,
            child: Text(item.label, style: LocoText.base(colors)),
          ),
      ],
    );
    if (selected != null) widget.onChanged?.call(selected);
  }
}

class LocoTextField extends StatelessWidget {
  const LocoTextField({
    super.key,
    required this.controller,
    this.placeholder,
    this.onSubmitted,
    this.maxLength,
    this.tabular = false,
    this.numeric = false,
  });

  final TextEditingController controller;
  final String? placeholder;
  final ValueChanged<String>? onSubmitted;
  final int? maxLength;
  final bool tabular;
  final bool numeric;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final style = LocoText.base(colors).copyWith(
      fontFeatures: tabular ? const [FontFeature.tabularFigures()] : null,
    );
    return Container(
      height: 32,
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(LocoBox.radius),
        border: Border.all(color: colors.lineStrong),
      ),
      child: TextField(
        controller: controller,
        maxLength: maxLength,
        style: style,
        cursorColor: colors.ink,
        cursorWidth: 1.5,
        keyboardType:
            numeric ? const TextInputType.numberWithOptions(decimal: true) : null,
        textInputAction: TextInputAction.go,
        onSubmitted: onSubmitted,
        decoration: InputDecoration(
          counterText: '',
          isDense: true,
          border: InputBorder.none,
          contentPadding: const EdgeInsets.symmetric(horizontal: 10),
          hintText: placeholder,
          hintStyle: style.copyWith(color: colors.muted),
        ),
      ),
    );
  }
}

class EmptyHint extends StatelessWidget {
  const EmptyHint(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 32),
      child: Center(
        child: Text(text, style: LocoText.body(colors, color: colors.muted)),
      ),
    );
  }
}
