package com.tchirek.locosaonduh.engine

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.sin
import kotlin.math.sqrt

// 波の端 帯を限りて 移し替ふ
internal class Sinc(step: Double) {
    private val taps: Int
    private val rows: FloatArray
    private val history: Array<FloatArray>
    private var write = 0
    private var phase = 0.0
    private var stepValue = step
    private val sample = FloatArray(2)
    private val produced = FloatArray(2)

    init {
        val stretch = step.coerceAtLeast(1.0)
        val half = ceil(HALF_TAPS * stretch).toInt()
        taps = half * 2
        val cutoff = PASSBAND / stretch
        val norm = besselI0(KAISER_BETA)
        rows = FloatArray((PHASES + 1) * taps)
        val weights = DoubleArray(taps)
        for (rowIndex in 0..PHASES) {
            val offset = rowIndex.toDouble() / PHASES
            for (index in 0 until taps) {
                val x = offset + (half - 1).toDouble() - index
                val u = x / half
                val window = if (abs(u) < 1.0) {
                    besselI0(KAISER_BETA * sqrt(1.0 - u * u)) / norm
                } else {
                    0.0
                }
                weights[index] = cutoff * sinc(cutoff * x) * window
            }
            // 一列の 重みを足して 一となす
            var sum = 0.0
            for (weight in weights) sum += weight
            for (index in 0 until taps) {
                rows[rowIndex * taps + index] = (weights[index] / sum).toFloat()
            }
        }
        history = Array(taps * 2) { FloatArray(2) }
    }

    fun setStep(value: Double) {
        stepValue = value
    }

    fun push(frame: FloatArray, out: FloatArray, offset: Int, capacity: Int): Int {
        var cursor = offset
        load(frame)
        while (phase < 1.0) {
            if (cursor + 2 > capacity) break
            convolve()
            out[cursor] = produced[0]
            out[cursor + 1] = produced[1]
            cursor += 2
            phase += stepValue
        }
        phase -= 1.0
        return cursor
    }

    fun consumeNeeded(): Boolean = phase >= 1.0

    // 要るだけを 泉に汲みて 一つ出す
    fun consume(frame: FloatArray) {
        load(frame)
        phase -= 1.0
    }

    fun produce(out: FloatArray) {
        convolve()
        out[0] = produced[0]
        out[1] = produced[1]
        phase += stepValue
    }

    // 二重なる 輪に書き置けば 窓続く
    private fun load(frame: FloatArray) {
        history[write][0] = frame[0]
        history[write][1] = frame[1]
        history[write + taps][0] = frame[0]
        history[write + taps][1] = frame[1]
        write = (write + 1) % taps
    }

    private fun convolve() {
        val position = phase * PHASES
        val row = position.toInt().coerceIn(0, PHASES - 1)
        val fraction = (position - row).toFloat()
        val early = row * taps
        val late = early + taps
        var left = 0.0f
        var right = 0.0f
        for (index in 0 until taps) {
            val a = rows[early + index]
            val b = rows[late + index]
            val weight = a + (b - a) * fraction
            val frame = history[write + index]
            left += weight * frame[0]
            right += weight * frame[1]
        }
        produced[0] = left
        produced[1] = right
    }

    companion object {
        private const val HALF_TAPS = 32
        private const val PHASES = 256
        private const val KAISER_BETA = 8.0
        private const val PASSBAND = 0.95

        private fun sinc(x: Double): Double =
            if (abs(x) < 1e-12) 1.0 else sin(PI * x) / (PI * x)

        private fun besselI0(x: Double): Double {
            val quarter = x * x / 4.0
            var term = 1.0
            var sum = 1.0
            for (k in 1 until 64) {
                term *= quarter / (k * k).toDouble()
                sum += term
                if (term < sum * 1e-17) break
            }
            return sum
        }
    }
}

// 片耳も 遠き調べも 四十八
internal class CaptureAdapter(channels: Int, sourceRate: Int) {
    private val channelCount = channels.coerceAtLeast(1)
    private val resampler = if (sourceRate != Wire.SAMPLE_RATE) {
        Sinc(sourceRate.toDouble() / Wire.SAMPLE_RATE)
    } else {
        null
    }
    private val pair = FloatArray(2)

    fun push(input: FloatArray, length: Int, out: FloatArray, outOffset: Int): Int {
        var cursor = outOffset
        var index = 0
        val right = minOf(channelCount, 2) - 1
        while (index + channelCount <= length) {
            pair[0] = input[index]
            pair[1] = input[index + right]
            val sinc = resampler
            if (sinc == null) {
                if (cursor + 2 > out.size) break
                out[cursor] = pair[0]
                out[cursor + 1] = pair[1]
                cursor += 2
            } else {
                cursor = sinc.push(pair, out, cursor, out.size)
            }
            index += channelCount
        }
        return cursor
    }
}

internal class PlaybackAdapter(outputRate: Int) {
    private val resampler = if (outputRate != Wire.SAMPLE_RATE) {
        Sinc(Wire.SAMPLE_RATE.toDouble() / outputRate)
    } else {
        null
    }
    private val source = FloatArray(2)
    private val scratch = FloatArray(2)

    // 一つづつ 汲みては返す 秋の水
    fun next(fill: () -> Unit): FloatArray {
        val sinc = resampler ?: run {
            fill()
            return source
        }
        while (sinc.consumeNeeded()) {
            fill()
            sinc.consume(source)
        }
        sinc.produce(scratch)
        return scratch
    }
}
