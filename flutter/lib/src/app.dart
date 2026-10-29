import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'bridge.dart';
import 'snapshot.dart';
import 'theme.dart';
import 'ui/mark.dart';
import 'ui/nav.dart';
import 'ui/receive_view.dart';
import 'ui/send_view.dart';
import 'ui/settings_view.dart';

// 秋の夜や 三つの戸を 巡り行く
class LocoApp extends StatefulWidget {
  const LocoApp({super.key, required this.bridge});

  final EngineBridge bridge;

  @override
  State<LocoApp> createState() => _LocoAppState();
}

class _ToastData {
  _ToastData(this.message, this.level);

  final String message;
  final NoticeLevel level;
}

class _LocoAppState extends State<LocoApp> with WidgetsBindingObserver {
  Snapshot? _snapshot;
  String? _fatal;
  LocoTab _tab = LocoTab.receive;
  List<Kind> _busy = const [];
  bool _scanning = false;
  bool _refreshing = false;
  String? _connecting;
  bool? _autostart;
  _ToastData? _toast;
  Timer? _toastTimer;
  String _lastNotice = '';
  final ScrollController _scroll = ScrollController();
  StreamSubscription<Snapshot>? _subscription;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _bootstrap();
    _subscription = widget.bridge.watch().listen(
      _apply,
      onError: (Object error) {
        _showToast(BridgeException.messageOf(error), NoticeLevel.warning);
      },
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _subscription?.cancel();
    _scroll.dispose();
    _toastTimer?.cancel();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _resync();
  }

  Future<void> _bootstrap() async {
    try {
      final snapshot = await widget.bridge.bootstrap();
      _apply(snapshot);
    } catch (error) {
      if (mounted) setState(() => _fatal = BridgeException.messageOf(error));
    }
  }

  void _resync() {
    widget.bridge.bootstrap().then(_apply).catchError((Object _) {});
    if (_tab == LocoTab.settings) _syncSettings();
  }

  void _syncSettings() {
    _attempt(widget.bridge.refreshAudio);
    _attempt(() async {
      final status = await widget.bridge.autostartStatus();
      if (mounted) setState(() => _autostart = status);
    });
  }

  // 呼び戻し 灯を見るたびに 帳合はす
  void _apply(Snapshot next) {
    if (!mounted) return;
    final current = _snapshot;
    if (current != null && next.revision < current.revision) return;
    setState(() => _snapshot = next);
    final message = next.notice?.message;
    if (message == null || message.isEmpty) {
      _lastNotice = '';
      return;
    }
    if (message != _lastNotice) {
      _lastNotice = message;
      _showToast(message, next.notice?.level ?? NoticeLevel.info);
    }
  }

  void _showToast(String message, [NoticeLevel level = NoticeLevel.error]) {
    if (!mounted) return;
    if (_toast?.message == message) return;
    setState(() => _toast = _ToastData(message, level));
    _toastTimer?.cancel();
    _toastTimer = Timer(const Duration(milliseconds: 4200), () {
      if (mounted) setState(() => _toast = null);
    });
  }

  // つまづきは 紅葉のごとく 札に散る
  Future<bool> _attempt(Future<void> Function() task) async {
    try {
      await task();
      return true;
    } catch (error) {
      _showToast(BridgeException.messageOf(error));
      return false;
    }
  }

  Future<void> _toggle(Kind kind, bool on) async {
    setState(() => _busy = [..._busy, kind]);
    await _attempt(
      () => on ? widget.bridge.startStream(kind) : widget.bridge.stopStream(kind),
    );
    if (mounted) {
      setState(() => _busy = _busy.where((item) => item != kind).toList());
    }
  }

  Future<void> _subscribe(String key) async {
    setState(() => _connecting = key);
    await _attempt(() => widget.bridge.subscribe(key));
    if (mounted) setState(() => _connecting = null);
  }

  Future<void> _scan() async {
    if (_scanning) return;
    setState(() => _scanning = true);
    await _attempt(() async {
      await widget.bridge.probeNetwork();
      await Future<void>.delayed(const Duration(milliseconds: 650));
    });
    if (mounted) setState(() => _scanning = false);
  }

  Future<void> _refresh() async {
    if (_refreshing) return;
    setState(() => _refreshing = true);
    await _attempt(widget.bridge.refreshAudio);
    if (mounted) setState(() => _refreshing = false);
  }

  Future<void> _setAutostart(bool next) async {
    final previous = _autostart;
    setState(() => _autostart = next);
    if (!await _attempt(() => widget.bridge.setAutostart(next))) {
      if (mounted) setState(() => _autostart = previous);
    }
  }

  void _changeTab(LocoTab next) {
    if (_tab == next) return;
    setState(() => _tab = next);
    if (_scroll.hasClients) _scroll.jumpTo(0);
    if (next == LocoTab.settings) _syncSettings();
  }

  @override
  Widget build(BuildContext context) {
    final colors = LocoColors.of(context);
    final snapshot = _snapshot;
    final size = MediaQuery.sizeOf(context);
    final compact = size.width < 560;

    return Scaffold(
      backgroundColor: colors.bg,
      resizeToAvoidBottomInset: true,
      body: SafeArea(
        child: Stack(
          children: [
            if (_fatal != null && snapshot == null)
              _center(colors, error: _fatal)
            else if (snapshot == null)
              _center(colors)
            else
              _shell(colors, snapshot, compact),
            if (_toast != null)
              Positioned(
                left: 0,
                right: 0,
                bottom: compact ? 76 : 20,
                child: Center(child: _toastBubble(colors, _toast!)),
              ),
          ],
        ),
      ),
    );
  }

  Widget _center(LocoColors colors, {String? error}) {
    if (error == null) {
      return Center(child: LocoMark(size: 40, writhing: true));
    }
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const LocoMark(size: 40),
            const SizedBox(height: 10),
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text('音频服务未启动', style: LocoText.h1(colors)),
            ),
            const SizedBox(height: 10),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              child: Text(
                error,
                textAlign: TextAlign.center,
                style: LocoText.meta(colors),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _shell(LocoColors colors, Snapshot snapshot, bool compact) {
    final tab = switch (_tab) {
      LocoTab.receive => ReceiveView(
          snapshot: snapshot,
          connecting: _connecting,
          scanning: _scanning,
          onSubscribe: _subscribe,
          onUnsubscribe: (key) => _attempt(() => widget.bridge.unsubscribe(key)),
          onScan: _scan,
          onProbe: (address) =>
              _attempt(() => widget.bridge.probeAddress(address)),
          onSettings: () => _changeTab(LocoTab.settings),
        ),
      LocoTab.send => SendView(
          snapshot: snapshot,
          busy: _busy,
          scanning: _scanning,
          onToggle: _toggle,
          onMicChange: (sourceId) =>
              _attempt(() => widget.bridge.setMic(sourceId)),
          onScan: _scan,
        ),
      LocoTab.settings => SettingsView(
          snapshot: snapshot,
          refreshing: _refreshing,
          autostart: _autostart,
          onSetAlias: (alias) =>
              _attempt(() => widget.bridge.setAlias(alias)),
          onSetOutput: (outputId) =>
              _attempt(() => widget.bridge.setOutput(outputId)),
          onSetPrefs: ({bool? restore, bool? background}) => _attempt(
            () => widget.bridge.setPrefs(restore: restore, background: background),
          ),
          onSetAutostart: _setAutostart,
          onRefresh: _refresh,
        ),
    };
    final listening = snapshot.listening.where((stream) => !stream.isStandby).length;
    final nav = LocoNav(
      tab: _tab,
      onTab: _changeTab,
      listening: listening,
      live: snapshot.broadcasts.isNotEmpty,
      compact: compact,
    );
    final main = Container(
      color: colors.bg,
      child: Scrollbar(
        controller: _scroll,
        thickness: 6,
        radius: const Radius.circular(3),
        child: SingleChildScrollView(
          controller: _scroll,
          child: tab,
        ),
      ),
    );
    return compact
        ? Column(
            children: [
              Expanded(child: main),
              nav,
            ],
          )
        : Row(
            children: [
              nav,
              Expanded(child: main),
            ],
          );
  }

  Widget _toastBubble(LocoColors colors, _ToastData toast) {
    final width = MediaQuery.sizeOf(context).width;
    return TweenAnimationBuilder<double>(
      duration: const Duration(milliseconds: 160),
      curve: Curves.easeOut,
      tween: Tween(begin: 0, end: 1),
      builder: (context, value, child) =>
          Opacity(opacity: value, child: child),
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: math.min(420, width - 32)),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
          decoration: BoxDecoration(
            color: colors.inverse,
            borderRadius: BorderRadius.circular(LocoBox.radius),
          ),
          child: Text(
            toast.message,
            style: LocoText.body(colors, color: colors.onInverse),
          ),
        ),
      ),
    );
  }
}
