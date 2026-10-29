import 'package:flutter/material.dart';

import '../icons.dart';
import '../snapshot.dart';
import '../theme.dart';
import 'bits.dart';

const String appVersion = '0.3.0';

// 名を書きて 戸札新たに 秋の宿
class SettingsView extends StatefulWidget {
  const SettingsView({
    super.key,
    required this.snapshot,
    required this.refreshing,
    required this.autostart,
    required this.onSetAlias,
    required this.onSetOutput,
    required this.onSetPrefs,
    required this.onSetAutostart,
    required this.onRefresh,
  });

  final Snapshot snapshot;
  final bool refreshing;
  final bool? autostart;
  final Future<bool> Function(String alias) onSetAlias;
  final ValueChanged<String?> onSetOutput;
  final void Function({bool? restore, bool? background}) onSetPrefs;
  final ValueChanged<bool> onSetAutostart;
  final VoidCallback onRefresh;

  @override
  State<SettingsView> createState() => _SettingsViewState();
}

class _SettingsViewState extends State<SettingsView> {
  late final TextEditingController _alias;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _alias = TextEditingController(text: widget.snapshot.identity.alias);
    _alias.addListener(_onTyping);
  }

  void _onTyping() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _alias.removeListener(_onTyping);
    _alias.dispose();
    super.dispose();
  }

  bool get _dirty => _alias.text.trim() != widget.snapshot.identity.alias;

  Future<void> _save() async {
    if (_saving || !_dirty) return;
    setState(() => _saving = true);
    final ok = await widget.onSetAlias(_alias.text);
    if (mounted) {
      setState(() {
        _saving = false;
        if (ok) _alias.text = _alias.text.trim();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final snapshot = widget.snapshot;
    final busyOutput = snapshot.listening.isNotEmpty || snapshot.pending.isNotEmpty;

    return LocoPage(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SectionHead(title: '本机'),
          _Group(
            children: [
              _Setting(
                label: '设备名',
                trailing: Row(
                  children: [
                    Expanded(
                      child: LocoTextField(
                        controller: _alias,
                        maxLength: 32,
                        onSubmitted: (_) => _save(),
                      ),
                    ),
                    if (_dirty) ...[
                      const SizedBox(width: 4),
                      LocoIconButton(
                        icon: LocoIcon.check,
                        label: '保存设备名',
                        iconSize: 17,
                        strokeWidth: 2.2,
                        onPressed: _saving || _alias.text.trim().isEmpty ? null : _save,
                      ),
                    ],
                  ],
                ),
                control: true,
              ),
            ],
          ),
          const SizedBox(height: 32),
          SectionHead(
            title: '音频',
            tools: LocoIconButton(
              icon: LocoIcon.refreshCw,
              label: '刷新设备',
              iconSize: 15,
              spinning: widget.refreshing,
              onPressed: widget.refreshing ? null : widget.onRefresh,
            ),
          ),
          _Group(
            children: [
              _Setting(
                label: '播放设备',
                small: busyOutput ? '收听中不可改' : null,
                control: true,
                trailing: SelectBox(
                  value: snapshot.selectedOutput ?? '',
                  disabled: busyOutput,
                  items: [
                    const SelectItem('', '跟随系统'),
                    for (final device in snapshot.outputs)
                      SelectItem(device.id, device.name),
                  ],
                  onChanged: (value) =>
                      widget.onSetOutput(value.isEmpty ? null : value),
                ),
              ),
              _Setting(
                label: '虚拟麦克风',
                small: 'Android 系统暂无虚拟声卡',
                trailing: LocoSwitch(
                  checked: false,
                  disabled: true,
                  label: '虚拟麦克风',
                  onChanged: null,
                ),
              ),
            ],
          ),
          const SizedBox(height: 32),
          SectionHead(title: '后台'),
          _Group(
            children: [
              _Setting(
                label: '开机自启',
                trailing: LocoSwitch(
                  checked: widget.autostart ?? true,
                  disabled: widget.autostart == null,
                  label: '开机自启',
                  onChanged: widget.onSetAutostart,
                ),
              ),
              _Setting(
                label: '启动时恢复收放',
                trailing: LocoSwitch(
                  checked: snapshot.prefs.restore,
                  label: '启动时恢复收放',
                  onChanged: (restore) => widget.onSetPrefs(restore: restore),
                ),
              ),
              _Setting(
                label: '离开应用后保持后台',
                trailing: LocoSwitch(
                  checked: snapshot.prefs.background,
                  label: '离开应用后保持后台',
                  onChanged: (background) =>
                      widget.onSetPrefs(background: background),
                ),
              ),
            ],
          ),
          const SizedBox(height: 32),
          SectionHead(title: '网络'),
          _Group(
            children: [
              _Setting(
                label: '地址',
                value: snapshot.network.ip != null
                    ? '${snapshot.network.ip}:${snapshot.network.port}'
                    : '未连接',
              ),
              _Setting(
                label: '网卡',
                value: snapshot.network.interfaceName ?? '—',
              ),
            ],
          ),
          Padding(
            padding: const EdgeInsets.only(top: 32),
            child: Center(
              child: Text(
                'locoSaonduh $appVersion',
                style: LocoText.meta(colors).copyWith(
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Group extends StatelessWidget {
  const _Group({required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(height: 1, color: colors.line),
        ...children,
      ],
    );
  }
}

class _Setting extends StatelessWidget {
  const _Setting({
    required this.label,
    this.small,
    this.trailing,
    this.value,
    this.control = false,
  });

  final String label;
  final String? small;
  final Widget? trailing;
  final String? value;
  final bool control;

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    return LayoutBuilder(
      builder: (context, constraints) {
        final controlWidth = (constraints.maxWidth * 0.6).clamp(0.0, 260.0);
        return Container(
          constraints: const BoxConstraints(minHeight: 52),
          decoration: BoxDecoration(
            border: Border(bottom: BorderSide(color: colors.line)),
          ),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      softWrap: false,
                      style: LocoText.base(colors),
                    ),
                    if (small != null) ...[
                      const SizedBox(height: 2),
                      Text(
                        small!,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        softWrap: false,
                        style: LocoText.small(colors),
                      ),
                    ],
                  ],
                ),
              ),
              const SizedBox(width: 16),
              if (value != null)
                Flexible(
                  child: Align(
                    alignment: Alignment.centerRight,
                    child: MetaText(value!),
                  ),
                )
              else if (trailing != null)
                control
                    ? SizedBox(width: controlWidth, child: trailing)
                    : trailing!,
            ],
          ),
        );
      },
    );
  }
}
