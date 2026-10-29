import 'package:flutter/material.dart';

import '../icons.dart';
import '../preview.dart';
import '../snapshot.dart';
import '../theme.dart';
import 'bits.dart';

// 二つの灯 点して声を 分け合はむ
class SendView extends StatelessWidget {
  const SendView({
    super.key,
    required this.snapshot,
    required this.busy,
    required this.scanning,
    required this.onToggle,
    required this.onMicChange,
    required this.onScan,
  });

  final Snapshot snapshot;
  final List<Kind> busy;
  final bool scanning;
  final void Function(Kind kind, bool on) onToggle;
  final ValueChanged<String> onMicChange;
  final VoidCallback onScan;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final byKind = {
      for (final item in snapshot.broadcasts) item.kind: item,
    };
    final mic = snapshot.sources.where((item) => item.id == snapshot.selectedMic);
    final fallback = snapshot.sources.where((item) => item.isDefault);
    final micSource = mic.isNotEmpty
        ? mic.first
        : fallback.isNotEmpty
            ? fallback.first
            : (snapshot.sources.isNotEmpty ? snapshot.sources.first : null);
    final audience = <String, _Audience>{};
    for (final broadcast in snapshot.broadcasts) {
      for (final listener in broadcast.listeners) {
        final entry = audience.putIfAbsent(
          listener.address,
          () => _Audience(listener.alias, listener.address),
        );
        entry.kinds.add(broadcast.kind);
        entry.bufferMs = entry.bufferMs > listener.bufferMs
            ? entry.bufferMs
            : listener.bufferMs;
      }
    }
    final echoRisk = byKind[Kind.system] != null &&
        byKind[Kind.system]!.excludesSelf == false &&
        snapshot.listening.any((stream) => !stream.isStandby);

    return LocoPage(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Builder(
            builder: (context) {
              final systemNote = echoRisk;
              final micNote = snapshot.sources.isEmpty;
              final systemBody = 132 + (systemNote ? 29.4 : 0.0);
              final micBody =
                  132 + (snapshot.sources.isNotEmpty ? 40.0 : (micNote ? 29.4 : 0.0));
              final tileHeight = ((systemBody > micBody ? systemBody : micBody) + 2).toDouble();
              return SizedBox(
                height: tileHeight,
                child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                child: _tile(
                  context: context,
                  colors: colors,
                  kind: Kind.system,
                  label: '系统声音',
                  icon: LocoIcon.volume2,
                  live: byKind[Kind.system],
                  note: echoRisk ? '可能回声' : null,
                  warning: true,
                  child: null,
                  micSource: micSource,
                  height: tileHeight,
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: _tile(
                  context: context,
                  colors: colors,
                  kind: Kind.microphone,
                  label: '麦克风',
                  icon: LocoIcon.mic,
                  live: byKind[Kind.microphone],
                  note: snapshot.sources.isEmpty ? '无可用麦克风' : null,
                  warning: false,
                  height: tileHeight,
                  child: snapshot.sources.isNotEmpty
                      ? Padding(
                          padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
                          child: SelectBox(
                            compact: true,
                            value: micSource?.id ?? '',
                            items: [
                              for (final option in snapshot.sources)
                                SelectItem(option.id, option.name),
                            ],
                            onChanged: onMicChange,
                          ),
                        )
                      : null,
                  micSource: micSource,
                ),
              ),
            ],
                ),
              );
            },
          ),
          const SizedBox(height: 32),
          if (snapshot.broadcasts.isNotEmpty) ...[
            SectionHead(title: '听众', count: audience.length),
            if (audience.isNotEmpty)
              LocoList(
                children: [
                  for (final person in audience.values)
                    LocoRow(
                      children: [
                        const DeviceIcon(platform: 'desktop'),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                person.alias,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                softWrap: false,
                                style: LocoText.strong(colors),
                              ),
                              const SizedBox(height: 2),
                              MetaText(person.address),
                            ],
                          ),
                        ),
                        Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            for (var index = 0; index < person.kinds.length; index++) ...[
                              if (index > 0) const SizedBox(width: 6),
                              Tooltip(
                                message: person.kinds[index].title,
                                child: LucideIcon(
                                  person.kinds[index] == Kind.system
                                      ? LocoIcon.volume2
                                      : LocoIcon.mic,
                                  size: 15,
                                  strokeWidth: 2,
                                  color: colors.muted,
                                ),
                              ),
                            ],
                          ],
                        ),
                        const SizedBox(width: 12),
                        SizedBox(
                          width: 44,
                          child: Text(
                            person.bufferMs > 0 ? '${person.bufferMs} ms' : '…',
                            textAlign: TextAlign.right,
                            style: LocoText.meta(colors).copyWith(
                              fontFeatures: const [FontFeature.tabularFigures()],
                            ),
                          ),
                        ),
                      ],
                    ),
                ],
              )
            else
              const EmptyHint('等待收听…'),
          ] else ...[
            SectionHead(
              title: '附近设备',
              count: snapshot.peers.length,
              tools: LocoIconButton(
                icon: LocoIcon.refreshCw,
                label: '扫描',
                iconSize: 15,
                spinning: scanning,
                onPressed: scanning ? null : onScan,
              ),
            ),
            if (snapshot.peers.isNotEmpty)
              LocoList(
                children: [
                  for (final peer in snapshot.peers)
                    LocoRow(
                      children: [
                        DeviceIcon(platform: peer.platform),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Text(
                            peer.alias,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            softWrap: false,
                            style: LocoText.strong(colors),
                          ),
                        ),
                        if (peer.streams.isNotEmpty)
                          Text(
                            '广播中',
                            style: LocoText.small(colors),
                          ),
                      ],
                    ),
                ],
              )
            else
              const EmptyHint('暂无其他设备'),
          ],
        ],
      ),
    );
  }

  Widget _tile({
    required BuildContext context,
    required LocoColors colors,
    required Kind kind,
    required String label,
    required LocoIcon icon,
    required OwnBroadcast? live,
    required String? note,
    required bool warning,
    required Widget? child,
    required AudioSource? micSource,
    required double height,
  }) {
    final pending = busy.contains(kind);
    final disabled = pending || (kind == Kind.microphone && micSource == null);
    final accent = live != null;
    return Container(
      height: height,
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(LocoBox.radius),
        border: Border.all(
          color: accent ? colors.accent : colors.line,
        ),
      ),
      child: CustomPaint(
        foregroundPainter:
            accent ? _AccentRing(colors.accent) : null,
        child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          _TileButton(
            onTap: disabled ? null : () => onToggle(kind, live == null),
            enabled: !disabled,
            child: SizedBox(
              height: 132,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(14, 14, 14, 12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        LucideIcon(
                          icon,
                          size: 22,
                          strokeWidth: 1.6,
                          color: accent ? colors.ink : colors.muted,
                        ),
                        const Spacer(),
                        if (pending)
                          SpinningBuilder(
                            spinning: true,
                            child: LucideIcon(
                              LocoIcon.refreshCw,
                              size: 14,
                              strokeWidth: 1.8,
                              color: colors.muted,
                            ),
                          )
                        else if (live != null)
                          Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              const LocoDot(),
                              const SizedBox(width: 6),
                              Text(
                                _elapsed(live.startedAtMs),
                                style: LocoText.small(colors, color: colors.ink),
                              ),
                            ],
                          ),
                      ],
                    ),
                    const Spacer(),
                    Text(
                      label,
                      style: LocoText.body(
                        colors,
                        weight: FontWeight.w600,
                      ),
                    ),
                    if (live != null) ...[
                      const SizedBox(height: 8),
                      LevelMeter(
                        level: live.level,
                        bars: 18,
                        tone: LevelTone.accent,
                      ),
                      const SizedBox(height: 8),
                      Row(
                        children: [
                          LucideIcon(
                            LocoIcon.users,
                            size: 12,
                            strokeWidth: 1.8,
                            color: colors.muted,
                          ),
                          const SizedBox(width: 4),
                          Text(
                            '${live.listeners.length}',
                            style: LocoText.small(colors),
                          ),
                          const SizedBox(width: 4),
                          Text('·', style: LocoText.small(colors)),
                          const SizedBox(width: 4),
                          Text(
                            '${live.bitrateKbps} kbps',
                            style: LocoText.small(colors),
                          ),
                        ],
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
          if (child != null)
            child
          else if (note != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 0, 14, 12),
              child: Row(
                children: [
                  if (warning) ...[
                    LucideIcon(
                      LocoIcon.triangleAlert,
                      size: 12,
                      strokeWidth: 1.8,
                      color: colors.ink,
                    ),
                    const SizedBox(width: 5),
                  ],
                  Flexible(
                    child: Text(
                      note,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: LocoText.small(
                        colors,
                        color: warning ? colors.ink : colors.muted,
                      ),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
      ),
    );
  }

  static String _elapsed(int startedAtMs) {
    final seconds = ((locoNowMs() - startedAtMs) / 1000).floor();
    final safe = seconds < 0 ? 0 : seconds;
    final hours = safe ~/ 3600;
    final minutes = (safe % 3600) ~/ 60;
    final rest = safe % 60;
    String two(int value) => value.toString().padLeft(2, '0');
    return hours > 0 ? '$hours:${two(minutes)}:${two(rest)}' : '${two(minutes)}:${two(rest)}';
  }
}

class _AccentRing extends CustomPainter {
  _AccentRing(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = color
      ..isAntiAlias = true;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(0.5, 0.5, size.width - 1, size.height - 1),
        const Radius.circular(3.5),
      ),
      paint,
    );
  }

  @override
  bool shouldRepaint(_AccentRing oldDelegate) => oldDelegate.color != color;
}

class _Audience {
  _Audience(this.alias, this.address);

  final String alias;
  final String address;
  final List<Kind> kinds = [];
  int bufferMs = 0;
}

class _TileButton extends StatefulWidget {
  const _TileButton({
    required this.onTap,
    required this.enabled,
    required this.child,
  });

  final VoidCallback? onTap;
  final bool enabled;
  final Widget child;

  @override
  State<_TileButton> createState() => _TileButtonState();
}

class _TileButtonState extends State<_TileButton> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor:
          widget.enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onTap,
        behavior: HitTestBehavior.opaque,
        child: Opacity(
          opacity: widget.enabled ? 1 : 0.45,
          child: Container(
            decoration: BoxDecoration(
              color: _hover ? Colors.transparent : Colors.transparent,
            ),
            child: widget.child,
          ),
        ),
      ),
    );
  }
}
