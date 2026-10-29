package com.tchirek.locosaonduh.engine

import org.json.JSONArray
import org.json.JSONObject
import java.net.InetAddress

// 秋の空 札の文言 数えけり
object Wire {
    const val APP = "locoSaonduh"
    const val VERSION = 1
    const val PORT = 53410
    const val SAMPLE_RATE = 48_000
    const val CHANNELS = 2
    const val FRAME_MS = 10
    const val FRAME_SAMPLES = (SAMPLE_RATE / 1000) * FRAME_MS
    const val HEADER_LEN = 28
    const val MAX_CONTROL = 8_192
    const val MAX_DATAGRAM = 1_500

    val MULTICAST: InetAddress = InetAddress.getByName("239.255.77.77")
    val MAGIC: ByteArray = byteArrayOf(0x4C, 0x53, 0x41, 0x75)
}

enum class SourceKind(val label: String) {
    SYSTEM("system"),
    MICROPHONE("microphone");

    companion object {
        fun parse(value: String?): SourceKind? = when (value) {
            "system" -> SYSTEM
            "microphone" -> MICROPHONE
            else -> null
        }
    }
}

class AudioPacket(
    val session: Int,
    val seq: Int,
    val timestamp: Int,
    val bitrateKbps: Int,
    val current: ByteArray,
    val previous: ByteArray,
)

class BroadcastAnnounce(
    val session: Int,
    val source: String,
    val sourceKind: String,
    val startedAtMs: Long,
    val listeners: Int = 0,
    val codec: CodecAnnounce = CodecAnnounce(),
)

class CodecAnnounce(
    val name: String = "opus",
    val sampleRate: Int = Wire.SAMPLE_RATE,
    val channels: Int = Wire.CHANNELS,
    val frameMs: Int = Wire.FRAME_MS,
)

class Announce(
    val id: String,
    val alias: String,
    val hostname: String,
    val platform: String,
    val port: Int,
    var broadcast: BroadcastAnnounce? = null,
    var streams: List<BroadcastAnnounce> = emptyList(),
) {
    val compatible: Boolean
        get() = true

    fun json(): JSONObject = JSONObject()
        .put("app", Wire.APP)
        .put("version", Wire.VERSION)
        .put("id", id)
        .put("alias", alias)
        .put("hostname", hostname)
        .put("platform", platform)
        .put("port", port)
        .put("broadcast", broadcast?.let { broadcastJson(it) } ?: JSONObject.NULL)
        .apply {
            if (streams.isNotEmpty()) {
                put("streams", JSONArray().apply { streams.forEach { put(broadcastJson(it)) } })
            }
        }

    companion object {
        fun broadcastJson(stream: BroadcastAnnounce): JSONObject = JSONObject()
            .put("session", stream.session)
            .put("source", stream.source)
            .put("sourceKind", stream.sourceKind)
            .put("startedAtMs", stream.startedAtMs)
            .put("listeners", stream.listeners)
            .put(
                "codec",
                JSONObject()
                    .put("name", stream.codec.name)
                    .put("sampleRate", stream.codec.sampleRate)
                    .put("channels", stream.codec.channels)
                    .put("frameMs", stream.codec.frameMs),
            )

        fun fromJson(raw: JSONObject): Announce? {
            if (raw.optString("app") != Wire.APP) return null
            if (raw.optInt("version", -1) != Wire.VERSION) return null
            val id = raw.optString("id")
            if (id.isBlank()) return null
            val announce = Announce(
                id = id,
                alias = raw.optString("alias", "locoSaonduh"),
                hostname = raw.optString("hostname", "unknown"),
                platform = raw.optString("platform", "unknown"),
                port = raw.optInt("port", Wire.PORT),
            )
            val single = raw.optJSONObject("broadcast")?.let { streamFrom(it) }
            val many = raw.optJSONArray("streams")
            announce.streams = if (many != null && many.length() > 0) {
                (0 until many.length()).mapNotNull { index ->
                    many.optJSONObject(index)?.let { streamFrom(it) }
                }
            } else {
                listOfNotNull(single)
            }
            announce.broadcast = announce.streams.firstOrNull { it.sourceKind == "system" }
                ?: announce.streams.firstOrNull()
            return announce
        }

        private fun streamFrom(raw: JSONObject): BroadcastAnnounce {
            val codec = raw.optJSONObject("codec")
            return BroadcastAnnounce(
                session = raw.optInt("session"),
                source = raw.optString("source", "音频"),
                sourceKind = raw.optString("sourceKind", "system"),
                startedAtMs = raw.optLong("startedAtMs"),
                listeners = raw.optInt("listeners"),
                codec = CodecAnnounce(
                    name = codec?.optString("name") ?: "opus",
                    sampleRate = codec?.optInt("sampleRate") ?: Wire.SAMPLE_RATE,
                    channels = codec?.optInt("channels") ?: Wire.CHANNELS,
                    frameMs = codec?.optInt("frameMs") ?: Wire.FRAME_MS,
                ),
            )
        }
    }
}

class Peer(
    val id: String,
    val alias: String,
    val platform: String,
    val address: InetAddress,
    val port: Int,
    val hostname: String,
    val streams: List<BroadcastAnnounce>,
    val lastSeen: Long,
) {
    fun stream(kind: String): BroadcastAnnounce? = streams.firstOrNull { it.sourceKind == kind }

    fun primary(): BroadcastAnnounce? = stream("system") ?: streams.firstOrNull()
}

sealed interface Control {
    class AnnounceControl(val announce: Announce) : Control
    class Probe(val id: String, val announce: Announce?) : Control
    class SubscribeControl(val id: String, val alias: String, val platform: String, val session: Int?) : Control
    class Subscribed(
        val session: Int,
        val sourceId: String,
        val sourceAlias: String,
        val source: String,
        val sourceKind: String,
        val sampleRate: Int,
        val channels: Int,
        val frameMs: Int,
    ) : Control
    class Unsubscribe(val session: Int) : Control
    class Ended(val session: Int, val reason: String) : Control
    class FeedbackControl(
        val session: Int,
        val received: Long,
        val lost: Long,
        val jitterUs: Long,
        val bufferMs: Int,
        val concealment: Long,
        val stable: Boolean,
    ) : Control

    fun json(): JSONObject = when (this) {
        is AnnounceControl -> envelope("announce", announce.json())
        is Probe -> envelope("probe", JSONObject().put("id", id).put("announce", announce?.json()))
        is SubscribeControl -> envelope(
            "subscribe",
            JSONObject().put("id", id).put("alias", alias).put("platform", platform).apply {
                if (session != null) put("session", session)
            },
        )
        is Subscribed -> envelope(
            "subscribed",
            JSONObject()
                .put("session", session)
                .put("sourceId", sourceId)
                .put("sourceAlias", sourceAlias)
                .put("source", source)
                .put("sourceKind", sourceKind)
                .put("sampleRate", sampleRate)
                .put("channels", channels)
                .put("frameMs", frameMs),
        )
        is Unsubscribe -> envelope("unsubscribe", JSONObject().put("session", session))
        is Ended -> envelope("ended", JSONObject().put("session", session).put("reason", reason))
        is FeedbackControl -> envelope(
            "feedback",
            JSONObject()
                .put("session", session)
                .put("received", received)
                .put("lost", lost)
                .put("jitterUs", jitterUs)
                .put("bufferMs", bufferMs)
                .put("concealment", concealment)
                .put("stable", stable),
        )
    }

    private fun envelope(type: String, data: JSONObject): JSONObject =
        JSONObject().put("type", type).put("data", data)

    companion object {
        // 手紙読む 影に灯ともし 夜の窓
        fun parse(bytes: ByteArray): Control? {
            val root = try {
                JSONObject(String(bytes, Charsets.UTF_8))
            } catch (_: Exception) {
                return null
            }
            val data = root.optJSONObject("data") ?: return null
            return when (root.optString("type")) {
                "announce" -> Announce.fromJson(data)?.let { AnnounceControl(it) }
                "probe" -> {
                    val id = data.optString("id")
                    if (id.isBlank()) null
                    else Probe(id, data.optJSONObject("announce")?.let { Announce.fromJson(it) })
                }
                "subscribe" -> SubscribeControl(
                    id = data.optString("id"),
                    alias = data.optString("alias", "locoSaonduh"),
                    platform = data.optString("platform", "android"),
                    session = if (data.has("session") && !data.isNull("session")) data.optInt("session") else null,
                )
                "subscribed" -> Subscribed(
                    session = data.optInt("session"),
                    sourceId = data.optString("sourceId"),
                    sourceAlias = data.optString("sourceAlias", "locoSaonduh"),
                    source = data.optString("source", "音频"),
                    sourceKind = data.optString("sourceKind", "system"),
                    sampleRate = data.optInt("sampleRate", Wire.SAMPLE_RATE),
                    channels = data.optInt("channels", Wire.CHANNELS),
                    frameMs = data.optInt("frameMs", Wire.FRAME_MS),
                )
                "unsubscribe" -> Unsubscribe(data.optInt("session"))
                "ended" -> Ended(data.optInt("session"), data.optString("reason", "已停止"))
                "feedback" -> FeedbackControl(
                    session = data.optInt("session"),
                    received = data.optLong("received"),
                    lost = data.optLong("lost"),
                    jitterUs = data.optLong("jitterUs"),
                    bufferMs = data.optInt("bufferMs"),
                    concealment = data.optLong("concealment"),
                    stable = data.optBoolean("stable"),
                )
                else -> null
            }
        }
    }
}

// 十ミリの 葉の落ちる音 包みけり
fun isAudioDatagram(bytes: ByteArray): Boolean {
    if (bytes.size < Wire.HEADER_LEN) return false
    for (index in Wire.MAGIC.indices) {
        if (bytes[index] != Wire.MAGIC[index]) return false
    }
    return true
}

fun buildAudioPacket(
    session: Int,
    seq: Int,
    timestamp: Int,
    bitrateKbps: Int,
    current: ByteArray,
    previous: ByteArray,
): ByteArray {
    val out = ByteArray(Wire.HEADER_LEN + current.size + previous.size)
    Wire.MAGIC.copyInto(out, 0)
    out[4] = 1
    out[5] = 1
    out[6] = 0
    out[7] = Wire.HEADER_LEN.toByte()
    putInt(out, 8, session)
    putInt(out, 12, seq)
    putInt(out, 16, timestamp)
    putShort(out, 20, current.size)
    putShort(out, 22, previous.size)
    putShort(out, 24, bitrateKbps)
    putShort(out, 26, 0)
    current.copyInto(out, Wire.HEADER_LEN)
    previous.copyInto(out, Wire.HEADER_LEN + current.size)
    return out
}

fun buildEndPacket(session: Int): ByteArray {
    val out = ByteArray(Wire.HEADER_LEN)
    Wire.MAGIC.copyInto(out, 0)
    out[4] = 1
    out[5] = 2
    out[6] = 0
    out[7] = Wire.HEADER_LEN.toByte()
    putInt(out, 8, session)
    return out
}

// 包み解く 匙ひとつに 秋の実を
fun parseAudioPacket(bytes: ByteArray): AudioPacket? {
    if (bytes.size < Wire.HEADER_LEN) return null
    if (!isAudioDatagram(bytes)) return null
    if (bytes[4].toInt() != 1) return null
    if (bytes[5].toInt() != 1) return null
    if (bytes[7].toInt() and 0xFF != Wire.HEADER_LEN) return null
    val session = getInt(bytes, 8)
    val seq = getInt(bytes, 12)
    val timestamp = getInt(bytes, 16)
    val currentLength = getShort(bytes, 20)
    val previousLength = getShort(bytes, 22)
    val bitrate = getShort(bytes, 24)
    if (currentLength <= 0) return null
    if (Wire.HEADER_LEN + currentLength + previousLength > bytes.size) return null
    val current = bytes.copyOfRange(Wire.HEADER_LEN, Wire.HEADER_LEN + currentLength)
    val previous = bytes.copyOfRange(
        Wire.HEADER_LEN + currentLength,
        Wire.HEADER_LEN + currentLength + previousLength,
    )
    return AudioPacket(session, seq, timestamp, bitrate, current, previous)
}

fun parseEndSession(bytes: ByteArray): Int? {
    if (bytes.size < Wire.HEADER_LEN) return null
    if (isAudioDatagram(bytes) && bytes[5].toInt() == 2) return getInt(bytes, 8)
    return null
}

private fun putInt(target: ByteArray, offset: Int, value: Int) {
    target[offset] = (value ushr 24).toByte()
    target[offset + 1] = (value ushr 16).toByte()
    target[offset + 2] = (value ushr 8).toByte()
    target[offset + 3] = value.toByte()
}

private fun putShort(target: ByteArray, offset: Int, value: Int) {
    target[offset] = (value ushr 8).toByte()
    target[offset + 1] = value.toByte()
}

fun getInt(bytes: ByteArray, offset: Int): Int =
    ((bytes[offset].toInt() and 0xFF) shl 24) or
        ((bytes[offset + 1].toInt() and 0xFF) shl 16) or
        ((bytes[offset + 2].toInt() and 0xFF) shl 8) or
        (bytes[offset + 3].toInt() and 0xFF)

fun getShort(bytes: ByteArray, offset: Int): Int =
    ((bytes[offset].toInt() and 0xFF) shl 8) or (bytes[offset + 1].toInt() and 0xFF)

fun streamKey(peerId: String, kind: SourceKind): String = "$peerId#${kind.label}"

fun parseKey(key: String): Pair<String, SourceKind>? {
    val index = key.lastIndexOf('#')
    if (index <= 0) return null
    val kind = SourceKind.parse(key.substring(index + 1)) ?: return null
    return key.substring(0, index) to kind
}
