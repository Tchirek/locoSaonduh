import 'package:flutter/services.dart';

import 'preview.dart';
import 'snapshot.dart';

// 呼び戻し 灯を見るたびに 帳合はす
class EngineBridge {
  EngineBridge({bool preview = kPreviewMode}) : _preview = preview;

  static const MethodChannel _method =
      MethodChannel('com.tchirek.locosaonduh/engine');
  static const EventChannel _events =
      EventChannel('com.tchirek.locosaonduh/state');

  final bool _preview;
  PreviewEngine? _previewEngine;

  Future<Snapshot> bootstrap() async {
    if (_preview) {
      _previewEngine ??= PreviewEngine();
      return _previewEngine!.snapshot();
    }
    final raw = await _method.invokeMethod<String>('bootstrap');
    return Snapshot.decode(raw!);
  }

  Stream<Snapshot> watch() {
    if (_preview) {
      final engine = _previewEngine ??= PreviewEngine();
      return engine.stream;
    }
    return _events.receiveBroadcastStream().map(
          (event) => Snapshot.decode(event as String),
        );
  }

  Future<void> startStream(Kind kind) =>
      _call('startStream', {'kind': kind.label});

  Future<void> stopStream(Kind kind) =>
      _call('stopStream', {'kind': kind.label});

  Future<void> subscribe(String key) => _call('subscribe', {'key': key});

  Future<void> unsubscribe(String key) => _call('unsubscribe', {'key': key});

  Future<void> setAlias(String alias) => _call('setAlias', {'alias': alias});

  Future<void> setMic(String sourceId) =>
      _call('setMic', {'sourceId': sourceId});

  Future<void> setOutput(String? outputId) =>
      _call('setOutput', {'outputId': outputId});

  Future<void> setPrefs({bool? restore, bool? background}) =>
      _call('setPrefs', {'restore': restore, 'background': background});

  Future<void> setVirtualMic(bool enabled) =>
      _call('setVirtualMic', {'enabled': enabled});

  Future<void> refreshAudio() => _call('refreshAudio');

  Future<void> probeNetwork() => _call('probeNetwork');

  Future<void> probeAddress(String address) =>
      _call('probeAddress', {'address': address});

  Future<bool> autostartStatus() async {
    if (_preview) return true;
    return await _method.invokeMethod<bool>('autostartStatus') ?? false;
  }

  Future<void> setAutostart(bool enabled) =>
      _call('setAutostart', {'enabled': enabled});

  Future<Map<String, dynamic>> platformInfo() async {
    if (_preview) {
      return const {'sdk': 35, 'systemCapture': true};
    }
    final raw = await _method.invokeMethod<Map<Object?, Object?>>('platformInfo');
    return Map<String, dynamic>.from(raw ?? const {});
  }

  Future<void> _call(String method, [Map<String, dynamic>? args]) async {
    if (_preview) {
      _previewEngine ??= PreviewEngine();
      _previewEngine!.handle(method, args ?? const {});
      return;
    }
    try {
      await _method.invokeMethod<void>(method, args);
    } on PlatformException catch (error) {
      throw BridgeException(error.message ?? '操作没有完成');
    } on MissingPluginException {
      throw const BridgeException('音频服务未启动');
    }
  }
}

class BridgeException implements Exception {
  const BridgeException(this.message);

  final String message;

  @override
  String toString() => message;

  static String messageOf(Object error) {
    if (error is BridgeException) return error.message;
    if (error is PlatformException) return error.message ?? '操作没有完成';
    if (error is String) return error;
    return '操作没有完成，请再试一次。';
  }
}
