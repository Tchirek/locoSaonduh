package com.tchirek.locosaonduh.engine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// 十ミリに 一つ届きて 一つ鳴る
class StreamBufferTest {
    private val base = 1_000_000_000L

    private fun at(ms: Long): Long = base + ms * 1_000_000L

    private fun frame(byte: Byte): ByteArray = ByteArray(40) { byte }

    private fun steady(buffer: StreamBuffer, from: Int, until: Int) {
        for (seq in from until until) {
            buffer.insertPair(seq, frame(seq.toByte()), ByteArray(0), at(seq.toLong() * 10))
            buffer.tick(at(seq.toLong() * 10))
        }
    }

    @Test
    fun startsAfterTargetAndPreservesOrder() {
        val buffer = StreamBuffer(base)
        for (seq in 0..3) {
            buffer.insertPair(seq, frame(seq.toByte()), ByteArray(0), base)
        }
        for (seq in 0..3) {
            val next = buffer.tick(base)!!
            assertEquals(seq.toLong(), next.seq)
            assertNotNull(next.payload)
        }
    }

    @Test
    fun redundancyFillsADroppedDatagram() {
        val buffer = StreamBuffer(base)
        buffer.insertPair(0, frame(0), ByteArray(0), base)
        buffer.insertPair(1, frame(1), frame(0), base)
        buffer.insertPair(3, frame(3), frame(2), base)
        buffer.insertPair(4, frame(4), frame(3), base)
        val frames = (0 until 4).mapNotNull { buffer.tick(base) }
        assertEquals(4, frames.size)
        assertTrue(frames.all { it.payload != null })
        assertEquals(0, buffer.counters().concealed)
    }

    @Test
    fun consecutiveGapsPauseThenRebuffer() {
        val buffer = StreamBuffer(base)
        for (seq in 0..3) buffer.insertPair(seq, frame(seq.toByte()), ByteArray(0), base)
        repeat(4) { assertNotNull(buffer.tick(base)!!.payload) }
        assertNull(buffer.tick(base)!!.payload)
        assertNull(buffer.tick(base)!!.payload)
        assertNull(buffer.tick(base))
        assertTrue(buffer.counters().paused)
        assertTrue(buffer.targetMs() > 40)
        val resumed = at(300)
        for (seq in 4 until 12) buffer.insertPair(seq, frame(seq.toByte()), ByteArray(0), resumed)
        assertNotNull(buffer.tick(resumed))
    }

    @Test
    fun aSteadyStreamMeasuresNoJitter() {
        val buffer = StreamBuffer(base)
        steady(buffer, 0, 1_000)
        assertTrue("jitter ${buffer.jitterMs()}", buffer.jitterMs() < 0.5)
        assertEquals(0, buffer.counters().concealed)
        assertTrue("target ${buffer.targetMs()}", buffer.targetMs() in 30..31)
    }

    @Test
    fun delaySpikesRaiseTheTarget() {
        val buffer = StreamBuffer(base)
        val arrivals = (0 until 600).map { seq ->
            val late = if (seq % 10 == 5) 45L else 0L
            (seq * 10L + late) to seq
        }.sortedBy { it.first }
        for ((ms, seq) in arrivals) {
            buffer.insertPair(seq, frame(1), ByteArray(0), at(ms))
            buffer.tick(at(ms))
        }
        assertTrue("target ${buffer.targetMs()}", buffer.targetMs() >= 55)
        assertTrue(buffer.jitterMs() > 1.0)
    }

    @Test
    fun sequenceNumbersSurviveWrapping() {
        val buffer = StreamBuffer(base)
        val start = Int.MAX_VALUE - 300
        for (index in 0 until 600) {
            val seq = start + index
            val now = at(index.toLong() * 10)
            buffer.insertPair(seq, frame(1), frame(1), now)
            buffer.tick(now)
        }
        val counters = buffer.counters()
        assertEquals(0, counters.concealed)
        assertTrue("played ${counters.played}", counters.played > 550)
        assertTrue(buffer.jitterMs() < 0.5)
    }

    @Test
    fun aStalledStreamSkipsStaleAudioOnResume() {
        val buffer = StreamBuffer(base)
        steady(buffer, 0, 100)
        for (tick in 100 until 140) buffer.tick(at(tick.toLong() * 10))
        assertTrue(buffer.counters().paused)
        val burst = at(1_400)
        for (seq in 100 until 140) buffer.insertPair(seq, frame(1), frame(1), burst)
        val first = buffer.tick(burst)!!
        val behindMs = ((140 - first.seq) * 10).toInt()
        assertTrue("${behindMs}ms behind", behindMs <= buffer.targetMs() + 10)
    }

    @Test
    fun qualityDegradesFastAndRecoversSlowly() {
        val quality = QualityController()
        val bad = Control.FeedbackControl(1, 900, 100, 30_000, 80, 100, false)
        var state = quality.state
        repeat(6) { state = quality.observe(listOf(bad)) }
        assertTrue(state.bitrateKbps <= 96)
        assertEquals(QualityTier.LEAN, state.tier)
        val good = Control.FeedbackControl(1, 1_000, 0, 2_000, 40, 0, true)
        repeat(12) { state = quality.observe(listOf(good)) }
        assertEquals(192, state.bitrateKbps)
        assertEquals(QualityTier.STUDIO, state.tier)
        assertFalse(state.lossPct > 0)
    }
}
