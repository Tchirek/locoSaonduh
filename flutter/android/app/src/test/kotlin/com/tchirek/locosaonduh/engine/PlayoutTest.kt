package com.tchirek.locosaonduh.engine

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// 仮の喉 正弦ひとつ 歌はせて
class PlayoutTest {
    private val base = 5_000_000_000L

    private fun at(ms: Long): Long = base + ms * 1_000_000L

    private fun payload(seq: Int): ByteArray = ByteArray(40) { seq.toByte() }

    @Test
    fun aSteadyStreamPlaysWithoutConcealment() {
        val buffer = StreamBuffer(base)
        var decoded = 0
        val decode: DecodeSink = { _, pcm ->
            for (index in pcm.indices) pcm[index] = 0.25f
            decoded += 1
            true
        }
        val playout = Playout()
        val block = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS)
        for (seq in 0 until 160) {
            val now = at(seq.toLong() * 10)
            val previous = if (seq == 0) ByteArray(0) else payload(seq - 1)
            synchronized(buffer) { buffer.insertPair(seq, payload(seq), previous, now) }
            if (seq >= 4) {
                synchronized(buffer) { playout.render(now, buffer, decode, block) }
            }
        }
        assertTrue("decoded $decoded", decoded > 0)
        assertEquals(0, buffer.counters().concealed)
        assertTrue(playout.queuedFrames >= 0)
    }

    @Test
    fun resamplingKeepsUnityGainOnceTheWindowIsFull() {
        val sinc = Sinc(1.0)
        val input = floatArrayOf(0.5f, -0.5f)
        val out = FloatArray(4)
        repeat(96) { sinc.push(input, out, 0, out.size) }
        val end = sinc.push(input, out, 0, out.size)
        assertEquals(2, end)
        assertEquals(0.5f, out[0], 1e-3f)
        assertEquals(-0.5f, out[1], 1e-3f)
    }

    @Test
    fun captureAdapterDuplicatesMonoIntoBothChannels() {
        val adapter = CaptureAdapter(1, Wire.SAMPLE_RATE)
        val input = floatArrayOf(0.25f, -0.5f)
        val out = FloatArray(8)
        val end = adapter.push(input, input.size, out, 0)
        assertEquals(4, end)
        assertEquals(0.25f, out[0], 0f)
        assertEquals(0.25f, out[1], 0f)
        assertEquals(-0.5f, out[2], 0f)
        assertEquals(-0.5f, out[3], 0f)
    }

    @Test
    fun theFrameQueueBoundedButDrainable() {
        val queue = FrameQueue(16)
        val frame = FloatArray(2) { 0.1f }
        repeat(50) { queue.pushInterleaved(frame, frame.size) }
        assertEquals(16, queue.size)
        val out = FloatArray(2)
        queue.popInto(out)
        assertEquals(0.1f, out[0], 0f)
        assertEquals(15, queue.size)
        queue.drain(15)
        assertEquals(0, queue.size)
    }

    @Test
    fun playbackAdapterPullsEnoughSourceFrames() {
        val adapter = PlaybackAdapter(44_100)
        val source = FloatArray(2)
        val out = adapter.next {
            source[0] = 0.5f
            source[1] = -0.5f
        }
        assertTrue(out.all { it.isFinite() })
        assertEquals(out.size, 2)
    }
}
