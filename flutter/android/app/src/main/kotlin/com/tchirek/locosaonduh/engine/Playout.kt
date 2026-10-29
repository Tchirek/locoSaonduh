package com.tchirek.locosaonduh.engine

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

// 継ぎ目をば 波の山にて 結び合ふ
internal class FrameQueue(capacityFrames: Int = 4_096) {
    private val mask: Int
    private val data: FloatArray
    private var head = 0
    private var count = 0

    init {
        var size = 1
        while (size < capacityFrames) size = size shl 1
        mask = size - 1
        data = FloatArray(size * 2)
    }

    val size: Int get() = count

    fun clear() {
        head = 0
        count = 0
    }

    fun pushInterleaved(source: FloatArray, length: Int) {
        var index = 0
        while (index + 2 <= length && count < mask + 1) {
            val slot = ((head + count) and mask) * 2
            data[slot] = source[index]
            data[slot + 1] = source[index + 1]
            count += 1
            index += 2
        }
    }

    fun popInto(out: FloatArray) {
        if (count == 0) {
            out[0] = 0f
            out[1] = 0f
            return
        }
        val slot = head * 2
        out[0] = data[slot]
        out[1] = data[slot + 1]
        head = (head + 1) and mask
        count -= 1
    }

    fun frameAt(index: Int, out: FloatArray) {
        val slot = ((head + index) and mask) * 2
        out[0] = data[slot]
        out[1] = data[slot + 1]
    }

    fun setFrameAt(index: Int, left: Float, right: Float) {
        val slot = ((head + index) and mask) * 2
        data[slot] = left
        data[slot + 1] = right
    }

    fun drain(frames: Int) {
        val drop = frames.coerceAtMost(count)
        head = (head + drop) and mask
        count -= drop
    }
}

internal typealias DecodeSink = (payload: ByteArray?, pcm: FloatArray) -> Boolean

// 時の差を 均して秋の 声繋ぐ
internal class Playout {
    private val fifo = FrameQueue()
    private val sinc = Sinc(1.0)
    private val pcm = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS)
    private val source = FloatArray(2)
    private val produced = FloatArray(2)
    private var levelMs: Double? = null
    private var drift = 0.0
    private var sounding = false
    private var lastSplice: Long? = null
    private var splices = 0L

    fun render(now: Long, buffer: StreamBuffer, decode: DecodeSink, out: FloatArray): Boolean {
        steer(now, buffer, decode)
        var cursor = 0
        while (cursor + 2 <= out.size) {
            while (sinc.consumeNeeded()) {
                if (fifo.size == 0) refill(now, buffer, decode)
                fifo.popInto(source)
                sinc.consume(source)
            }
            sinc.produce(produced)
            out[cursor] = produced[0]
            out[cursor + 1] = produced[1]
            cursor += 2
        }
        return sounding
    }

    private fun steer(now: Long, buffer: StreamBuffer, decode: DecodeSink) {
        if (!buffer.isPlaying()) {
            levelMs = null
            sinc.setStep(1.0 + drift)
            return
        }
        val level = buffer.bufferedMs() + fifo.size / SAMPLES_PER_MS
        val target = buffer.latencyTargetMs()
        val smooth = levelMs ?: level.also { levelMs = it }
        val smoothed = smooth + (level - smooth) * LEVEL_SMOOTHING
        levelMs = smoothed
        val error = smoothed - target
        // 移ろひを 積みて時計の 癖を知る
        if (abs(error) < DRIFT_WINDOW_MS) {
            val seconds = Wire.FRAME_MS.toDouble() / 1_000.0
            drift = (drift + error * DRIFT_PER_MS_S * seconds).coerceIn(-MAX_SKEW, MAX_SKEW)
        }
        val skew = (error * SKEW_PER_MS + drift).coerceIn(-MAX_SKEW, MAX_SKEW)
        sinc.setStep(1.0 + skew)

        val since = lastSplice
        val due = since == null || now - since >= SPLICE_EVERY_NANOS
        if (due && level - target > SPLICE_ABOVE_MS && error > SPLICE_ABOVE_MS / 2.0) {
            val removed = splice(now, buffer, decode)
            if (removed != null) {
                lastSplice = now
                splices += 1
                levelMs = (levelMs ?: 0.0) - removed / SAMPLES_PER_MS
            }
        }
    }

    private fun splice(now: Long, buffer: StreamBuffer, decode: DecodeSink): Int? {
        val need = Wire.FRAME_SAMPLES + SEEK + OVERLAP * 2
        var guard = 0
        while (fifo.size < need) {
            if (!refill(now, buffer, decode) || guard++ > 64) return null
        }
        val early = FloatArray(OVERLAP * 2)
        var templateEnergy = 0.0f
        for (index in 0 until OVERLAP) {
            fifo.frameAt(index, source)
            early[index * 2] = source[0]
            early[index * 2 + 1] = source[1]
            val mono = source[0] + source[1]
            templateEnergy += mono * mono
        }
        var bestLag = Wire.FRAME_SAMPLES
        var bestScore = -Float.MAX_VALUE
        val late = FloatArray(2)
        for (lag in (Wire.FRAME_SAMPLES - SEEK)..(Wire.FRAME_SAMPLES + SEEK)) {
            var dot = 0.0f
            var energy = 0.0f
            for (index in 0 until OVERLAP) {
                fifo.frameAt(lag + index, late)
                val earlyMono = early[index * 2] + early[index * 2 + 1]
                val lateMono = late[0] + late[1]
                dot += earlyMono * lateMono
                energy += lateMono * lateMono
            }
            val score = dot / sqrt(templateEnergy * energy).coerceAtLeast(1e-9f)
            if (score > bestScore) {
                bestScore = score
                bestLag = lag
            }
        }
        val equalPower = bestScore < 0.5f
        for (index in 0 until OVERLAP) {
            val t = (index.toFloat() + 0.5f) / OVERLAP
            val rise: Float
            val fall: Float
            if (equalPower) {
                rise = sin(t * (PI / 2).toFloat())
                fall = cos(t * (PI / 2).toFloat())
            } else {
                rise = 0.5f - 0.5f * cos((PI * t).toFloat())
                fall = 1.0f - rise
            }
            fifo.frameAt(index, source)
            fifo.frameAt(bestLag + index, late)
            fifo.setFrameAt(
                bestLag + index,
                source[0] * fall + late[0] * rise,
                source[1] * fall + late[1] * rise,
            )
        }
        fifo.drain(bestLag)
        return bestLag
    }

    // 声の無を 器に満たして 次を待つ
    private fun refill(now: Long, buffer: StreamBuffer, decode: DecodeSink): Boolean {
        val frame = buffer.tick(now)
        if (frame != null) {
            if (!decode(frame.payload, pcm) && !decode(null, pcm)) pcm.fill(0f)
            if (!sounding) fadeIn(pcm)
            sounding = true
            fifo.pushInterleaved(pcm, pcm.size)
            return true
        }
        if (sounding) {
            if (!decode(null, pcm)) pcm.fill(0f)
            fadeOut(pcm)
            sounding = false
            fifo.pushInterleaved(pcm, pcm.size)
            return false
        }
        pcm.fill(0f)
        fifo.pushInterleaved(pcm, pcm.size)
        return false
    }

    val queuedFrames: Int get() = fifo.size

    val spliceCount: Long get() = splices

    companion object {
        private const val SAMPLES_PER_MS = Wire.SAMPLE_RATE / 1_000.0
        private const val MAX_SKEW = 0.0025
        private const val SKEW_PER_MS = 1e-4
        private const val DRIFT_PER_MS_S = 1e-5
        private const val DRIFT_WINDOW_MS = 20.0
        private const val LEVEL_SMOOTHING = 0.01
        private const val SPLICE_ABOVE_MS = 30.0
        private const val SPLICE_EVERY_NANOS = 250_000_000L
        private const val OVERLAP = 240
        private const val SEEK = 120
        private const val FADE = 240
    }
}

// 目覚めには 小声で始む 朝の鳥
private fun fadeIn(pcm: FloatArray) {
    for (index in 0 until 240) {
        val gain = raisedCosine(index, 240)
        pcm[index * 2] *= gain
        pcm[index * 2 + 1] *= gain
    }
}

// 散る際は 声を細めて 闇に入る
private fun fadeOut(pcm: FloatArray) {
    val span = pcm.size / 2
    for (index in 0 until span) {
        val gain = 1.0f - raisedCosine(index, span)
        pcm[index * 2] *= gain
        pcm[index * 2 + 1] *= gain
    }
}

private fun raisedCosine(index: Int, span: Int): Float =
    0.5f - 0.5f * cos((PI * (index + 0.5) / span).toFloat())
