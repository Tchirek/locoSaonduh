import 'dart:async';
import 'dart:convert';

import 'snapshot.dart';

const bool kPreviewMode = bool.fromEnvironment('LOCO_PREVIEW');

const int previewNowMs = 1760000000000;

int locoNowMs() =>
    kPreviewMode ? previewNowMs : DateTime.now().millisecondsSinceEpoch;

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

  Snapshot snapshot() => Snapshot.fromJson(
        Map<String, dynamic>.from(jsonDecode(jsonEncode(_state)) as Map),
      );

  void handle(String method, Map<String, dynamic> args) {
    final broadcasts =
        (_state['broadcasts'] as List).cast<Map<String, dynamic>>();
    final listening =
        (_state['listening'] as List).cast<Map<String, dynamic>>();
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
        'revision': 1,
        'identity': {'id': 'preview-self', 'alias': 'TCHIREK-PC'},
        'network': {
          'ip': '192.168.31.42',
          'interface': 'WLAN',
          'port': 53410,
        },
        'peers': [
          {
            'id': 'preview-mara',
            'alias': 'Mara 的 MacBook',
            'platform': 'macos',
            'address': '192.168.31.18',
            'streams': [
              {'source': '系统声音', 'sourceKind': 'system'},
              {'source': 'MacBook 麦克风', 'sourceKind': 'microphone'},
            ],
            'isListening': true,
          },
          {
            'id': 'preview-mini',
            'alias': '客厅 · Mac mini',
            'platform': 'macos',
            'address': '192.168.31.27',
            'streams': [
              {'source': '系统声音', 'sourceKind': 'system'},
            ],
            'isListening': false,
          },
          {
            'id': 'preview-pixel',
            'alias': 'Pixel 9',
            'platform': 'android',
            'address': '192.168.31.53',
            'streams': [],
            'isListening': false,
          },
        ],
        'broadcasts': [
          {
            'session': 31337,
            'kind': 'system',
            'source': '系统声音',
            'startedAtMs': previewNowMs - 754000,
            'listeners': [
              {'alias': 'Mara 的 MacBook', 'address': '192.168.31.18', 'bufferMs': 40},
              {'alias': 'Pixel 9', 'address': '192.168.31.53', 'bufferMs': 40},
            ],
            'bitrateKbps': 160,
            'level': 0.58,
            'excludesSelf': true,
          },
          {
            'session': 31338,
            'kind': 'microphone',
            'source': '麦克风阵列 (Realtek Audio)',
            'startedAtMs': previewNowMs - 61000,
            'listeners': [
              {'alias': 'Mara 的 MacBook', 'address': '192.168.31.18', 'bufferMs': 40},
            ],
            'bitrateKbps': 112,
            'level': 0.22,
            'excludesSelf': true,
          },
        ],
        'listening': [
          {
            'key': 'preview-mara#system',
            'senderId': 'preview-mara',
            'senderAlias': 'Mara 的 MacBook',
            'kind': 'system',
            'source': '系统声音',
            'state': 'live',
            'bufferMs': 40,
            'lossPct': 0.0,
            'level': 0.3,
            'output': '扬声器 (Realtek Audio)',
            'toVirtual': false,
          },
          {
            'key': 'preview-pixel#microphone',
            'senderId': 'preview-pixel',
            'senderAlias': 'Pixel 9',
            'kind': 'microphone',
            'source': '麦克风',
            'state': 'standby',
            'bufferMs': 0,
            'lossPct': 0.0,
            'level': 0.0,
            'output': '扬声器 (Realtek Audio)',
            'toVirtual': false,
          },
        ],
        'pending': <String>[],
        'sources': [
          {
            'id': 'mic|preview-array',
            'name': '麦克风阵列 (Realtek Audio)',
            'kind': 'microphone',
            'isDefault': true,
          },
          {
            'id': 'mic|preview-usb',
            'name': 'USB 麦克风',
            'kind': 'microphone',
            'isDefault': false,
          },
        ],
        'outputs': [
          {
            'id': 'wasapi:preview-output',
            'name': '扬声器 (Realtek Audio)',
            'isDefault': true,
          },
          {
            'id': 'wasapi:preview-cable',
            'name': 'CABLE Input (VB-Audio Virtual Cable)',
            'isDefault': false,
          },
        ],
        'selectedMic': 'mic|preview-array',
        'selectedOutput': null,
        'virtualMic': {
          'enabled': false,
          'state': 'missing',
          'device': null,
          'provider': 'VB-CABLE',
          'link': 'https://vb-audio.com/Cable/',
        },
        'prefs': {'restore': true, 'background': true},
        'notice': null,
      };
}
