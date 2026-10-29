import 'dart:async';
import 'dart:convert';

import 'snapshot.dart';

const bool kPreviewMode = bool.fromEnvironment('LOCO_PREVIEW');

// 絵空事 畳に映す 秋の灯
class PreviewEngine {
  PreviewEngine();

  final _controller = StreamController<Snapshot>.broadcast();
  Timer? _ticker;
  Map<String, dynamic> _state = _fixture();

  Stream<Snapshot> get stream {
    _ticker ??= Timer.periodic(const Duration(milliseconds: 180), (_) {
      _controller.add(snapshot());
    });
    return _controller.stream;
  }

  Snapshot snapshot() =>
      Snapshot.fromJson(Map<String, dynamic>.from(jsonDecode(jsonEncode(_state)) as Map));

  void handle(String method, Map<String, dynamic> args) {
    final broadcasts = (_state['broadcasts'] as List).cast<Map<String, dynamic>>();
    final listening = (_state['listening'] as List).cast<Map<String, dynamic>>();
    switch (method) {
      case 'startStream':
        break;
      case 'stopStream':
        broadcasts.removeWhere((item) => item['kind'] == args['kind']);
      case 'subscribe':
        break;
      case 'unsubscribe':
        listening.removeWhere((item) => item['key'] == args['key']);
      case 'setAlias':
        (_state['identity'] as Map<String, dynamic>)['alias'] = args['alias'];
      case 'setMic':
        _state['selectedMic'] = args['sourceId'];
      case 'setOutput':
        _state['selectedOutput'] = args['outputId'];
      case 'setPrefs':
        final prefs = _state['prefs'] as Map<String, dynamic>;
        if (args['restore'] != null) prefs['restore'] = args['restore'];
        if (args['background'] != null) prefs['background'] = args['background'];
      case 'probeAddress':
        break;
    }
    _state['revision'] = ((_state['revision'] as num) + 1).toInt();
    _controller.add(snapshot());
  }

  static Map<String, dynamic> _fixture() => {
        'revision': 42,
        'identity': {'id': 'preview', 'alias': 'Tchirek 的书房'},
        'network': {'ip': '192.168.1.24', 'interface': 'wlan0', 'port': 53410},
        'peers': [
          {
            'id': 'desk',
            'alias': '书房主机',
            'platform': 'windows',
            'address': '192.168.1.12',
            'streams': [
              {'source': '系统声音', 'sourceKind': 'system'},
            ],
            'isListening': true,
          },
          {
            'id': 'pixel',
            'alias': 'Kai 的 Pixel',
            'platform': 'android',
            'address': '192.168.1.31',
            'streams': [
              {'source': '内置麦克风', 'sourceKind': 'microphone'}
            ],
            'isListening': true,
          },
          {
            'id': 'mac',
            'alias': '客厅 Mac',
            'platform': 'macos',
            'address': '192.168.1.18',
            'streams': [],
            'isListening': false,
          },
          {
            'id': 'pi',
            'alias': '树莓派',
            'platform': 'linux',
            'address': '192.168.1.9',
            'streams': [],
            'isListening': false,
          },
        ],
        'broadcasts': [
          {
            'session': 9001,
            'kind': 'system',
            'source': '系统声音',
            'startedAtMs': DateTime.now().millisecondsSinceEpoch - 754000,
            'listeners': [
              {'alias': 'Kai 的 Pixel', 'address': '192.168.1.31', 'bufferMs': 42},
              {'alias': '客厅 Mac', 'address': '192.168.1.18', 'bufferMs': 68},
            ],
            'bitrateKbps': 160,
            'level': 0.58,
            'excludesSelf': true,
          },
          {
            'session': 9002,
            'kind': 'microphone',
            'source': '内置麦克风',
            'startedAtMs': DateTime.now().millisecondsSinceEpoch - 61000,
            'listeners': [
              {'alias': 'Kai 的 Pixel', 'address': '192.168.1.31', 'bufferMs': 42},
            ],
            'bitrateKbps': 96,
            'level': 0.22,
            'excludesSelf': true,
          },
        ],
        'listening': [
          {
            'key': 'desk#system',
            'senderId': 'desk',
            'senderAlias': '书房主机',
            'kind': 'system',
            'source': '系统声音',
            'state': 'live',
            'bufferMs': 40,
            'lossPct': 0.0,
            'level': 0.54,
            'output': '扬声器',
            'toVirtual': false,
          },
          {
            'key': 'pixel#microphone',
            'senderId': 'pixel',
            'senderAlias': 'Kai 的 Pixel',
            'kind': 'microphone',
            'source': '内置麦克风',
            'state': 'buffering',
            'bufferMs': 0,
            'lossPct': 0.0,
            'level': 0.0,
            'output': '扬声器',
            'toVirtual': false,
          },
        ],
        'pending': ['mac#system'],
        'sources': [
          {'id': 'mic|15|-', 'name': '内置麦克风', 'kind': 'microphone', 'isDefault': true},
          {'id': 'mic|26|AA:BB:CC:DD:EE:FF', 'name': '蓝牙耳麦', 'kind': 'microphone', 'isDefault': false},
        ],
        'outputs': [
          {'id': 'out|2|-', 'name': '扬声器', 'isDefault': true},
          {'id': 'out|7|-', 'name': '蓝牙音频', 'isDefault': false},
          {'id': 'out|4|-', 'name': 'USB 音频', 'isDefault': false},
        ],
        'selectedMic': 'mic|15|-',
        'selectedOutput': null,
        'prefs': {'restore': true, 'background': true},
        'notice': null,
      };
}
