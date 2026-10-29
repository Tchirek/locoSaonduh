import 'package:flutter/material.dart';

import '../icons.dart';
import '../snapshot.dart';
import '../theme.dart';
import 'bits.dart';
import 'mark.dart';

// 鳴く虫も 黙せる虫も 一列に
class ReceiveView extends StatefulWidget {
  const ReceiveView({
    super.key,
    required this.snapshot,
    required this.connecting,
    required this.scanning,
    required this.onSubscribe,
    required this.onUnsubscribe,
    required this.onScan,
    required this.onProbe,
    required this.onSettings,
  });

  final Snapshot snapshot;
  final String? connecting;
  final bool scanning;
  final ValueChanged<String> onSubscribe;
  final ValueChanged<String> onUnsubscribe;
  final VoidCallback onScan;
  final Future<bool> Function(String address) onProbe;
  final VoidCallback onSettings;

  @override
  State<ReceiveView> createState() => _ReceiveViewState();
}

class _Lane {
  _Lane({
    required this.kind,
    required this.key,
    required this.source,
    required this.offered,
    this.stream,
  });

  final Kind kind;
  final String key;
  final String source;
  final bool offered;
  final ListeningStream? stream;
}

class _Entry {
  _Entry({
    required this.id,
    required this.alias,
    required this.platform,
    required this.lanes,
  });

  final String id;
  final String alias;
  final String platform;
  final List<_Lane> lanes;
}

class _ReceiveViewState extends State<ReceiveView> {
  final _probeController = TextEditingController();
  bool _probeOpen = false;
  bool _probing = false;

  @override
  void dispose() {
    _probeController.dispose();
    super.dispose();
  }

  List<_Entry> _entries() {
    final snapshot = widget.snapshot;
    final streams = {for (final stream in snapshot.listening) stream.key: stream};
    final list = <_Entry>[];
    for (final peer in snapshot.peers) {
      final lanes = <_Lane>[];
      for (final kind in Kind.values) {
        final key = streamKeyOf(peer.id, kind);
        final offer = peer.streams.where((item) => item.sourceKind == kind).toList();
        final stream = streams.remove(key);
        if (offer.isEmpty && stream == null) continue;
        lanes.add(
          _Lane(
            kind: kind,
            key: key,
            source: offer.isNotEmpty ? offer.first.source : (stream?.source ?? ''),
            offered: offer.isNotEmpty,
            stream: stream,
          ),
        );
      }
      if (lanes.isNotEmpty) {
        list.add(
          _Entry(
            id: peer.id,
            alias: peer.alias,
            platform: peer.platform,
            lanes: lanes,
          ),
        );
      }
    }
    for (final stream in streams.values) {
      final lane = _Lane(
        kind: stream.kind,
        key: stream.key,
        source: stream.source,
        offered: false,
        stream: stream,
      );
      final known = list.where((entry) => entry.id == stream.senderId);
      if (known.isNotEmpty) {
        known.first.lanes.add(lane);
      } else {
        list.add(
          _Entry(
            id: stream.senderId,
            alias: stream.senderAlias,
            platform: 'desktop',
            lanes: [lane],
          ),
        );
      }
    }
    return list;
  }

  String _laneTitle(_Lane lane) {
    final stream = lane.stream;
    if (stream == null) return lane.source;
    if (stream.isStandby) return '${lane.source} · 等待恢复';
    return '${lane.source} → ${stream.output}';
  }

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final snapshot = widget.snapshot;
    final entries = _entries();
    final output = snapshot.outputs.where((item) => item.id == snapshot.selectedOutput);
    final fallback = snapshot.outputs.where((item) => item.isDefault);
    final outputDevice = output.isNotEmpty
        ? output.first
        : fallback.isNotEmpty
            ? fallback.first
            : (snapshot.outputs.isNotEmpty ? snapshot.outputs.first : null);
    final idle = snapshot.peers
        .where((peer) => peer.streams.isEmpty && !peer.isListening)
        .length;

    return LocoPage(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.only(bottom: 28),
            child: Row(
              children: [
                LocoMark(
                  size: 40,
                  writhing: widget.scanning || entries.isEmpty,
                ),
                const SizedBox(width: 14),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        snapshot.identity.alias,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        softWrap: false,
                        style: LocoText.h1(colors),
                      ),
                      const SizedBox(height: 2),
                      MetaText(
                        snapshot.network.ip != null
                            ? '${snapshot.network.ip}:${snapshot.network.port}'
                            : '未连接网络',
                      ),
                    ],
                  ),
                ),
                if (outputDevice != null)
                  Flexible(
                    child: _OutputLabel(
                      name: outputDevice.name,
                      onTap: widget.onSettings,
                      compact: MediaQuery.sizeOf(context).width < 480,
                    ),
                  ),
              ],
            ),
          ),
          SectionHead(
            title: '附近广播',
            count: entries.length,
            tools: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                LocoIconButton(
                  icon: LocoIcon.plus,
                  label: '按 IP 添加',
                  active: _probeOpen,
                  onPressed: () => setState(() => _probeOpen = !_probeOpen),
                ),
                const SizedBox(width: 2),
                LocoIconButton(
                  icon: LocoIcon.refreshCw,
                  label: '扫描',
                  iconSize: 15,
                  spinning: widget.scanning,
                  onPressed: widget.scanning ? null : widget.onScan,
                ),
              ],
            ),
          ),
          if (_probeOpen)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Row(
                children: [
                  Expanded(
                    child: LocoTextField(
                      controller: _probeController,
                      placeholder: '192.168.1.24',
                      tabular: true,
                      numeric: true,
                      onSubmitted: (_) => _submitProbe(),
                    ),
                  ),
                  const SizedBox(width: 8),
                  LocoButton(
                    _probing ? '查找中' : '查找',
                    onPressed: _probing || _probeController.text.trim().isEmpty
                        ? null
                        : _submitProbe,
                  ),
                ],
              ),
            ),
          if (entries.isNotEmpty)
            LocoList(
              children: [
                for (final entry in entries)
                  LocoRow(
                    wrap: MediaQuery.sizeOf(context).width < 480,
                    padding: const EdgeInsets.symmetric(vertical: 10),
                    children: [
                      DeviceIcon(platform: entry.platform),
                      const SizedBox(width: 12),
                      if (MediaQuery.sizeOf(context).width < 480) ...[
                        Expanded(child: _title(colors, entry.alias)),
                      ] else ...[
                        Expanded(child: _title(colors, entry.alias)),
                      ],
                      _lanes(entry),
                    ],
                  ),
              ],
            )
          else
            const EmptyHint('暂无广播'),
          if (idle > 0)
            Padding(
              padding: const EdgeInsets.only(top: 12),
              child: Center(
                child: MetaText('另有 $idle 台设备在线'),
              ),
            ),
        ],
      ),
    );
  }

  Widget _title(LocoColors colors, String alias) => Text(
        alias,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        softWrap: false,
        style: LocoText.strong(colors),
      );

  Widget _lanes(_Entry entry) {
    final colors = LocoColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(left: 12),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var index = 0; index < entry.lanes.length; index++) ...[
            if (index > 0) const SizedBox(width: 6),
            _laneChip(colors, entry.lanes[index]),
          ],
        ],
      ),
    );
  }

  Widget _laneChip(LocoColors colors, _Lane lane) {
    final waiting = widget.connecting == lane.key ||
        widget.snapshot.pending.contains(lane.key) ||
        lane.stream?.state == StreamState.buffering;
    final on = lane.stream != null;
    final enabled = on || (lane.offered && !waiting);
    final icon = waiting
        ? LocoIcon.refreshCw
        : lane.kind == Kind.system
            ? LocoIcon.volume2
            : LocoIcon.mic;
    return Tooltip(
      message: _laneTitle(lane),
      waitDuration: const Duration(milliseconds: 500),
      child: _LaneChip(
        label: lane.kind.title,
        icon: icon,
        spinning: waiting,
        enabled: enabled,
        active: on,
        onTap: enabled
            ? () => on ? widget.onUnsubscribe(lane.key) : widget.onSubscribe(lane.key)
            : null,
      ),
    );
  }

  Future<void> _submitProbe() async {
    final address = _probeController.text.trim();
    if (_probing || address.isEmpty) return;
    setState(() => _probing = true);
    final ok = await widget.onProbe(address);
    if (mounted) {
      setState(() {
        _probing = false;
        if (ok) {
          _probeController.clear();
          _probeOpen = false;
        }
      });
    }
  }
}

class _LaneChip extends StatefulWidget {
  const _LaneChip({
    required this.label,
    required this.icon,
    required this.spinning,
    required this.enabled,
    required this.active,
    required this.onTap,
  });

  final String label;
  final LocoIcon icon;
  final bool spinning;
  final bool enabled;
  final bool active;
  final VoidCallback? onTap;

  @override
  State<_LaneChip> createState() => _LaneChipState();
}

class _LaneChipState extends State<_LaneChip> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final color = !widget.enabled
        ? colors.muted
        : widget.active || _hover
            ? colors.ink
            : colors.muted;
    return MouseRegion(
      cursor: widget.enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onTap,
        child: Opacity(
          opacity: widget.enabled ? 1 : 0.45,
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 140),
            height: 30,
            padding: const EdgeInsets.symmetric(horizontal: 10),
            decoration: BoxDecoration(
              color: widget.active ? colors.hover : Colors.transparent,
              borderRadius: BorderRadius.circular(LocoBox.radius),
              border: Border.all(
                color: widget.active || (_hover && widget.enabled)
                    ? colors.ink
                    : colors.lineStrong,
              ),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SpinningBuilder(
                  spinning: widget.spinning,
                  child: LucideIcon(
                    widget.icon,
                    size: 14,
                    strokeWidth: 1.8,
                    color: color,
                  ),
                ),
                const SizedBox(width: 6),
                Text(
                  widget.label,
                  style: LocoText.body(colors, size: 12, color: color),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _OutputLabel extends StatefulWidget {
  const _OutputLabel({
    required this.name,
    required this.onTap,
    required this.compact,
  });

  final String name;
  final VoidCallback onTap;
  final bool compact;

  @override
  State<_OutputLabel> createState() => _OutputLabelState();
}

class _OutputLabelState extends State<_OutputLabel> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    if (widget.compact) return const SizedBox.shrink();
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        onTap: widget.onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 260),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              LucideIcon(
                LocoIcon.volume2,
                size: 14,
                strokeWidth: 1.8,
                color: _hover ? colors.ink : colors.muted,
              ),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  widget.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  softWrap: false,
                  style: LocoText.meta(colors)
                      .copyWith(color: _hover ? colors.ink : colors.muted),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
