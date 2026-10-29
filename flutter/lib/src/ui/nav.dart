import 'package:flutter/material.dart';

import '../icons.dart';
import '../snapshot.dart';
import '../theme.dart';
import 'bits.dart';

// 三つの戸 行き来するたび 灯ともる
class LocoNav extends StatelessWidget {
  const LocoNav({
    super.key,
    required this.tab,
    required this.onTab,
    required this.listening,
    required this.live,
    required this.compact,
  });

  final LocoTab tab;
  final ValueChanged<LocoTab> onTab;
  final int listening;
  final bool live;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final items = [
      (LocoTab.receive, '接收', LocoIcon.headphones),
      (LocoTab.send, '发送', LocoIcon.radio),
      (LocoTab.settings, '设置', LocoIcon.settings),
    ];
    final children = [
      for (final item in items)
        _NavItem(
          label: item.$2,
          icon: item.$3,
          active: tab == item.$1,
          onTap: () => onTab(item.$1),
          trailing: item.$1 == LocoTab.receive && listening > 0
              ? _CountBadge(listening)
              : item.$1 == LocoTab.send && live
                  ? const LocoDot()
                  : null,
        ),
    ];
    if (compact) {
      return Container(
        width: double.infinity,
        padding: const EdgeInsets.fromLTRB(8, 6, 8, 8),
        decoration: BoxDecoration(
          border: Border(top: BorderSide(color: colors.line)),
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceAround,
          children: children,
        ),
      );
    }
    return Container(
      width: 72,
      padding: const EdgeInsets.symmetric(vertical: 16),
      decoration: BoxDecoration(
        border: Border(right: BorderSide(color: colors.line)),
      ),
      child: Column(
        children: [
          for (var index = 0; index < children.length; index++) ...[
            children[index],
            if (index != children.length - 1) const SizedBox(height: 4),
          ],
        ],
      ),
    );
  }
}

class _NavItem extends StatefulWidget {
  const _NavItem({
    required this.label,
    required this.icon,
    required this.active,
    required this.onTap,
    this.trailing,
  });

  final String label;
  final LocoIcon icon;
  final bool active;
  final VoidCallback onTap;
  final Widget? trailing;

  @override
  State<_NavItem> createState() => _NavItemState();
}

class _NavItemState extends State<_NavItem> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final color = widget.active || _hover ? colors.ink : colors.muted;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onTap,
        behavior: HitTestBehavior.opaque,
        child: SizedBox(
          width: 64,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                AnimatedContainer(
                  duration: const Duration(milliseconds: 140),
                  width: 52,
                  height: 30,
                  decoration: BoxDecoration(
                    color: widget.active
                        ? colors.lineStrong
                        : _hover
                            ? colors.hover
                            : Colors.transparent,
                    borderRadius: BorderRadius.circular(15),
                  ),
                  child: Center(
                    child: LucideIcon(
                      widget.icon,
                      size: 20,
                      strokeWidth: 1.7,
                      color: color,
                    ),
                  ),
                ),
                const SizedBox(height: 4),
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      widget.label,
                      style: LocoText.body(
                        colors,
                        size: 12,
                        weight: widget.active ? FontWeight.w600 : FontWeight.w400,
                        color: color,
                      ),
                    ),
                    if (widget.trailing != null) ...[
                      const SizedBox(width: 4),
                      widget.trailing!,
                    ],
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _CountBadge extends StatelessWidget {
  const _CountBadge(this.count);

  final int count;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Text(
      '$count',
      style: LocoText.body(
        colors,
        size: 12,
        weight: FontWeight.w600,
        color: colors.accent,
      ).copyWith(fontFeatures: const [FontFeature.tabularFigures()]),
    );
  }
}
