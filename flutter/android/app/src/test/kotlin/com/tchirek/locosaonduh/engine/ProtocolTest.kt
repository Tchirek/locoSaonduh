package com.tchirek.locosaonduh.engine

import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// 秋の空 札の文言 数えけり
class ProtocolTest {
    @Test
    fun audioRoundTripKeepsBothFrames() {
        val current = ByteArray(91) { 7 }
        val previous = ByteArray(88) { 3 }
        val buffer = buildAudioPacket(42, 17, 34_560, 128, current, previous)
        val packet = parseAudioPacket(buffer)!!
        assertEquals(42, packet.session)
        assertEquals(17, packet.seq)
        assertEquals(34_560, packet.timestamp)
        assertEquals(128, packet.bitrateKbps)
        assertArrayEquals(current, packet.current)
        assertArrayEquals(previous, packet.previous)
        assertEquals(Wire.HEADER_LEN + current.size + previous.size, buffer.size)
    }

    @Test
    fun malformedLengthsAreRejected() {
        val buffer = buildAudioPacket(1, 2, 3, 96, byteArrayOf(1, 2, 3), ByteArray(0))
        buffer[20] = 0x03.toByte()
        buffer[21] = 0xE7.toByte()
        assertNull(parseAudioPacket(buffer))
    }

    @Test
    fun endPacketsDoNotParseAsAudio() {
        val buffer = buildEndPacket(9)
        assertEquals(9, parseEndSession(buffer))
        assertNull(parseAudioPacket(buffer))
    }

    @Test
    fun jsonIsNotAnAudioDatagram() {
        assertFalse(isAudioDatagram("{\"type\":\"announce\"}".toByteArray()))
        assertTrue(isAudioDatagram(buildAudioPacket(1, 1, 0, 112, byteArrayOf(1), ByteArray(0))))
    }

    @Test
    fun probeRoundTripsThroughJson() {
        val announce = Announce(
            id = "device-a",
            alias = "书房",
            hostname = "desk",
            platform = "android",
            port = 53_410,
        )
        val json = Control.Probe("device-a", announce).json()
        assertEquals("probe", json.getString("type"))
        assertEquals("device-a", json.getJSONObject("data").getString("id"))
        val decoded = Control.parse(json.toString().toByteArray()) as Control.Probe
        assertEquals("device-a", decoded.id)
        assertEquals("android", decoded.announce!!.platform)
    }

    @Test
    fun anOldProbeWithoutAnnounceStillParses() {
        val decoded = Control.parse("{\"type\":\"probe\",\"data\":{\"id\":\"old\"}}".toByteArray())
        assertTrue(decoded is Control.Probe && decoded.announce == null)
    }

    @Test
    fun twoStreamsStayReadableToAOneStreamPeer() {
        val streams = listOf(
            BroadcastAnnounce(7, "麦克风", "microphone", 1),
            BroadcastAnnounce(9, "系统声音", "system", 1),
        )
        val announce = Announce("a", "A", "host", "android", 53_410).apply {
            this.streams = streams
            broadcast = streams.first { it.sourceKind == "system" }
        }
        val json = JSONObject(Control.AnnounceControl(announce).json().toString())
        assertEquals(9, json.getJSONObject("data").getJSONObject("broadcast").getInt("session"))
        val control = Control.parse(json.toString().toByteArray()) as Control.AnnounceControl
        assertEquals(2, control.announce.streams.size)
        assertEquals(9, control.announce.broadcast!!.session)
    }

    @Test
    fun aSubscribeWithoutSessionStillParses() {
        val decoded = Control.parse(
            "{\"type\":\"subscribe\",\"data\":{\"id\":\"x\",\"alias\":\"X\",\"platform\":\"android\"}}"
                .toByteArray(),
        ) as Control.SubscribeControl
        assertNull(decoded.session)
    }

    @Test
    fun streamKeysPairDeviceAndKind() {
        assertEquals("abc#system", streamKey("abc", SourceKind.SYSTEM))
        assertEquals("abc#microphone", streamKey("abc", SourceKind.MICROPHONE))
        val parsed = parseKey("abc#microphone")!!
        assertEquals("abc", parsed.first)
        assertEquals(SourceKind.MICROPHONE, parsed.second)
        assertNull(parseKey("abc"))
        assertNull(parseKey("abc#speaker"))
        assertNotEquals(0, buildEndPacket(1)[8].toInt() xor 1)
    }
}
