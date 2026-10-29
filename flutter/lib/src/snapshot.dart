import 'dart:convert';

enum Kind {
  system('system', '系统声音'),
  microphone('microphone', '麦克风');

  const Kind(this.label, this.title);

  final String label;
  final String title;

  static Kind parse(String value) =>
      value == 'microphone' ? Kind.microphone : Kind.system;
}

String streamKeyOf(String peerId, Kind kind) => '$peerId#${kind.label}';

class Snapshot {
  const Snapshot({
    required this.revision,
    required this.identity,
    required this.network,
    required this.peers,
    required this.broadcasts,
    required this.listening,
    required this.pending,
    required this.sources,
    required this.outputs,
    required this.selectedMic,
    required this.selectedOutput,
    required this.prefs,
    required this.notice,
  });

  final int revision;
  final Identity identity;
  final NetworkInfo network;
  final List<Peer> peers;
  final List<OwnBroadcast> broadcasts;
  final List<ListeningStream> listening;
  final List<String> pending;
  final List<AudioSource> sources;
  final List<OutputDevice> outputs;
  final String? selectedMic;
  final String? selectedOutput;
  final Prefs prefs;
  final Notice? notice;

  factory Snapshot.fromJson(Map<String, dynamic> json) {
    List<T> list<T>(String key, T Function(Map<String, dynamic>) parse) =>
        ((json[key] as List?) ?? const [])
            .map((item) => parse(Map<String, dynamic>.from(item as Map)))
            .toList();
    return Snapshot(
      revision: (json['revision'] as num?)?.toInt() ?? 0,
      identity: Identity.fromJson(
        Map<String, dynamic>.from(json['identity'] as Map? ?? const {}),
      ),
      network: NetworkInfo.fromJson(
        Map<String, dynamic>.from(json['network'] as Map? ?? const {}),
      ),
      peers: list('peers', Peer.fromJson),
      broadcasts: list('broadcasts', OwnBroadcast.fromJson),
      listening: list('listening', ListeningStream.fromJson),
      pending: ((json['pending'] as List?) ?? const []).cast<String>(),
      sources: list('sources', AudioSource.fromJson),
      outputs: list('outputs', OutputDevice.fromJson),
      selectedMic: json['selectedMic'] as String?,
      selectedOutput: json['selectedOutput'] as String?,
      prefs: Prefs.fromJson(
        Map<String, dynamic>.from(json['prefs'] as Map? ?? const {}),
      ),
      notice: json['notice'] == null
          ? null
          : Notice.fromJson(Map<String, dynamic>.from(json['notice'] as Map)),
    );
  }

  factory Snapshot.decode(String raw) =>
      Snapshot.fromJson(Map<String, dynamic>.from(jsonDecode(raw) as Map));
}

class Identity {
  const Identity({required this.id, required this.alias});

  final String id;
  final String alias;

  factory Identity.fromJson(Map<String, dynamic> json) => Identity(
        id: json['id'] as String? ?? '',
        alias: json['alias'] as String? ?? '',
      );
}

class NetworkInfo {
  const NetworkInfo({this.ip, this.interfaceName, required this.port});

  final String? ip;
  final String? interfaceName;
  final int port;

  factory NetworkInfo.fromJson(Map<String, dynamic> json) => NetworkInfo(
        ip: json['ip'] as String?,
        interfaceName: json['interface'] as String?,
        port: (json['port'] as num?)?.toInt() ?? 53410,
      );
}

class Peer {
  const Peer({
    required this.id,
    required this.alias,
    required this.platform,
    required this.address,
    required this.streams,
    required this.isListening,
  });

  final String id;
  final String alias;
  final String platform;
  final String address;
  final List<PeerStream> streams;
  final bool isListening;

  factory Peer.fromJson(Map<String, dynamic> json) => Peer(
        id: json['id'] as String? ?? '',
        alias: json['alias'] as String? ?? '',
        platform: json['platform'] as String? ?? '',
        address: json['address'] as String? ?? '',
        streams: ((json['streams'] as List?) ?? const [])
            .map((item) =>
                PeerStream.fromJson(Map<String, dynamic>.from(item as Map)))
            .toList(),
        isListening: json['isListening'] as bool? ?? false,
      );
}

class PeerStream {
  const PeerStream({required this.source, required this.sourceKind});

  final String source;
  final Kind sourceKind;

  factory PeerStream.fromJson(Map<String, dynamic> json) => PeerStream(
        source: json['source'] as String? ?? '',
        sourceKind: Kind.parse(json['sourceKind'] as String? ?? 'system'),
      );
}

class OwnBroadcast {
  const OwnBroadcast({
    required this.session,
    required this.kind,
    required this.source,
    required this.startedAtMs,
    required this.listeners,
    required this.bitrateKbps,
    required this.level,
    required this.excludesSelf,
  });

  final int session;
  final Kind kind;
  final String source;
  final int startedAtMs;
  final List<Listener> listeners;
  final int bitrateKbps;
  final double level;
  final bool excludesSelf;

  factory OwnBroadcast.fromJson(Map<String, dynamic> json) => OwnBroadcast(
        session: (json['session'] as num?)?.toInt() ?? 0,
        kind: Kind.parse(json['kind'] as String? ?? 'system'),
        source: json['source'] as String? ?? '',
        startedAtMs: (json['startedAtMs'] as num?)?.toInt() ?? 0,
        listeners: ((json['listeners'] as List?) ?? const [])
            .map((item) =>
                Listener.fromJson(Map<String, dynamic>.from(item as Map)))
            .toList(),
        bitrateKbps: (json['bitrateKbps'] as num?)?.toInt() ?? 0,
        level: (json['level'] as num?)?.toDouble() ?? 0,
        excludesSelf: json['excludesSelf'] as bool? ?? false,
      );
}

class Listener {
  const Listener({
    required this.alias,
    required this.address,
    required this.bufferMs,
  });

  final String alias;
  final String address;
  final int bufferMs;

  factory Listener.fromJson(Map<String, dynamic> json) => Listener(
        alias: json['alias'] as String? ?? '',
        address: json['address'] as String? ?? '',
        bufferMs: (json['bufferMs'] as num?)?.toInt() ?? 0,
      );
}

enum StreamState { buffering, live, waiting, standby }

class ListeningStream {
  const ListeningStream({
    required this.key,
    required this.senderId,
    required this.senderAlias,
    required this.kind,
    required this.source,
    required this.state,
    required this.bufferMs,
    required this.lossPct,
    required this.level,
    required this.output,
    required this.toVirtual,
  });

  final String key;
  final String senderId;
  final String senderAlias;
  final Kind kind;
  final String source;
  final StreamState state;
  final int bufferMs;
  final double lossPct;
  final double level;
  final String output;
  final bool toVirtual;

  bool get isStandby => state == StreamState.standby;

  factory ListeningStream.fromJson(Map<String, dynamic> json) =>
      ListeningStream(
        key: json['key'] as String? ?? '',
        senderId: json['senderId'] as String? ?? '',
        senderAlias: json['senderAlias'] as String? ?? '',
        kind: Kind.parse(json['kind'] as String? ?? 'system'),
        source: json['source'] as String? ?? '',
        state: switch (json['state'] as String?) {
          'live' => StreamState.live,
          'waiting' => StreamState.waiting,
          'standby' => StreamState.standby,
          _ => StreamState.buffering,
        },
        bufferMs: (json['bufferMs'] as num?)?.toInt() ?? 0,
        lossPct: (json['lossPct'] as num?)?.toDouble() ?? 0,
        level: (json['level'] as num?)?.toDouble() ?? 0,
        output: json['output'] as String? ?? '',
        toVirtual: json['toVirtual'] as bool? ?? false,
      );
}

class AudioSource {
  const AudioSource({
    required this.id,
    required this.name,
    required this.kind,
    required this.isDefault,
  });

  final String id;
  final String name;
  final Kind kind;
  final bool isDefault;

  factory AudioSource.fromJson(Map<String, dynamic> json) => AudioSource(
        id: json['id'] as String? ?? '',
        name: json['name'] as String? ?? '',
        kind: Kind.parse(json['kind'] as String? ?? 'microphone'),
        isDefault: json['isDefault'] as bool? ?? false,
      );
}

class OutputDevice {
  const OutputDevice({
    required this.id,
    required this.name,
    required this.isDefault,
  });

  final String id;
  final String name;
  final bool isDefault;

  factory OutputDevice.fromJson(Map<String, dynamic> json) => OutputDevice(
        id: json['id'] as String? ?? '',
        name: json['name'] as String? ?? '',
        isDefault: json['isDefault'] as bool? ?? false,
      );
}

class Prefs {
  const Prefs({required this.restore, required this.background});

  final bool restore;
  final bool background;

  factory Prefs.fromJson(Map<String, dynamic> json) => Prefs(
        restore: json['restore'] as bool? ?? true,
        background: json['background'] as bool? ?? true,
      );

  Prefs copyWith({bool? restore, bool? background}) => Prefs(
        restore: restore ?? this.restore,
        background: background ?? this.background,
      );
}

enum NoticeLevel { info, warning, error }

class Notice {
  const Notice({required this.level, required this.message});

  final NoticeLevel level;
  final String message;

  factory Notice.fromJson(Map<String, dynamic> json) => Notice(
        level: switch (json['level'] as String?) {
          'warning' => NoticeLevel.warning,
          'error' => NoticeLevel.error,
          _ => NoticeLevel.info,
        },
        message: json['message'] as String? ?? '',
      );
}

enum LocoTab { receive, send, settings }
