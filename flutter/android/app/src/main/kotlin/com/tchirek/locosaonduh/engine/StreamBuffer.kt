package com.tchirek.locosaonduh.engine

import java.util.ArrayDeque
import java.util.TreeMap
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.roundToInt

// 遅れ来る 雁を数へて 宿を取る
internal class DelayEstimator {
    private var originAt = 0L
    private var originSeq = 0L
    private var hasOrigin = false
    private val floor = ArrayDeque<LongArray>()
    private val histogram = DoubleArray(DELAY_BUCKETS)
    private var mass = 0.0
    private var lastTransit: Double? = null
    var jitterMs = 0.0
        private set

    private fun transit(seq: Long, now: Long): Double {
        if (!hasOrigin) {
            hasOrigin = true
            originAt = now
            originSeq = seq
        }
        val sentMs = (seq - originSeq).toDouble() * Wire.FRAME_MS
        return (now - originAt) / 1_000_000.0 - sentMs
    }

    fun observe(seq: Long, now: Long) {
        val transit = transit(seq, now)
        lastTransit?.let { last ->
            jitterMs += (abs(transit - last) - jitterMs) / 16.0
        }
        lastTransit = transit
        val scaled = (transit * 1000.0).toLong()
        while (floor.peekLast()?.let { it[1] >= scaled } == true) floor.pollLast()
        floor.addLast(longArrayOf(now, scaled))
        while (floor.peekFirst()?.let { now - it[0] > FLOOR_WINDOW_NANOS } == true) floor.pollFirst()
    }

    fun record(seq: Long, now: Long) {
        val front = floor.peekFirst() ?: return
        val low = front[1] / 1000.0
        val relative = (transit(seq, now) - low).coerceAtLeast(0.0)
        val bucket = relative.toInt().coerceIn(0, DELAY_BUCKETS - 1)
        for (index in histogram.indices) histogram[index] *= DELAY_FORGET
        histogram[bucket] += 1.0 - DELAY_FORGET
        mass = mass * DELAY_FORGET + (1.0 - DELAY_FORGET)
    }

    fun quantile(q: Double): Double? {
        if (mass < 0.2) return null
        val goal = mass * q
        var sum = 0.0
        for (index in histogram.indices) {
            sum += histogram[index]
            if (sum >= goal) return index.toDouble() + 1.0
        }
        return DELAY_BUCKETS.toDouble()
    }

    companion object {
        private const val DELAY_BUCKETS = 256
        private const val DELAY_FORGET = 0.995
        private const val FLOOR_WINDOW_NANOS = 2_000_000_000L
    }
}

enum class Insert { NEW, DUPLICATE, TOO_LATE, OUT_OF_RANGE }

class PlayoutFrame(val seq: Long, val payload: ByteArray?)

// 十ミリを 積みて静かに 待ちけり
internal class StreamBuffer(now: Long) {
    private val packets = TreeMap<Long, ByteArray>()
    private var playhead: Long? = null
    private var newest: Long? = null
    private var targetMsValue = START_TARGET_MS
    private val delay = DelayEstimator()
    private var started = false
    private var paused = false
    private var firstArrival: Long? = now
    private var lastArrival: Long? = null
    private var lastTick: Long? = null
    private var received = 0L
    private var recovered = 0L
    private var concealed = 0L
    private var played = 0L
    private var successiveConcealments = 0
    private var cleanSince: Long? = now
    private var pausedSince: Long? = null

    fun insertPair(seqValue: Int, current: ByteArray, previous: ByteArray, now: Long) {
        val seq = extend(seqValue) ?: return
        delay.observe(seq, now)
        when (insertOne(seq, current, now)) {
            Insert.NEW, Insert.TOO_LATE -> delay.record(seq, now)
            Insert.DUPLICATE, Insert.OUT_OF_RANGE -> {}
        }
        if (previous.isNotEmpty() && seq > 0) {
            if (insertOne(seq - 1, previous, now) == Insert.NEW) {
                recovered += 1
                delay.record(seq - 1, now)
            }
        }
        delay.quantile(DELAY_QUANTILE)?.let { measured ->
            targetMsValue = targetMsValue.coerceAtLeast(desiredTarget(measured))
        }
        lastArrival = now
    }

    // 巡り来て 数の尽きても 先を知る
    private fun extend(seq: Int): Long? {
        val last = newest ?: run {
            newest = seq.toLong()
            return seq.toLong()
        }
        val step = seq - (last and 0xFFFF_FFFFL).toInt()
        val extended = last + step.toLong()
        if (extended < 0) return null
        newest = maxOf(last, extended)
        return extended
    }

    private fun insertOne(seq: Long, payload: ByteArray, now: Long): Insert {
        playhead?.let { if (seq < it) return Insert.TOO_LATE }
        if (packets.containsKey(seq)) return Insert.DUPLICATE
        playhead?.let { if (seq > it + 120) return Insert.OUT_OF_RANGE }
        packets[seq] = payload
        received += 1
        if (firstArrival == null) firstArrival = now
        if (packets.size > 96) {
            val keepFrom = playhead ?: packets.firstKey()
            val inRange = packets.tailMap(keepFrom, true).size
            val drop = inRange - 80
            if (drop > 0) {
                val iterator = packets.tailMap(keepFrom, true).keys.iterator()
                repeat(drop) {
                    if (iterator.hasNext()) {
                        iterator.next()
                        iterator.remove()
                        concealed += 1
                    }
                }
            }
        }
        return Insert.NEW
    }

    private fun targetFrames(): Int = ceil(targetMsValue / Wire.FRAME_MS).toInt()

    private fun canStart(now: Long): Boolean {
        val first = firstArrival ?: return false
        val waited = now - first
        val buffered = packets.size
        return buffered >= targetFrames() ||
            (buffered > 0 && waited >= startupGrace(targetFrames()))
    }

    // 野分過ぎ 古き便りは 読み飛ばす
    private fun resume(now: Long) {
        val skip = (packets.size - maxOf(targetFrames(), 1)).coerceAtLeast(0)
        playhead = packets.keys.elementAtOrNull(skip)
        playhead?.let { discardBefore(it) }
        paused = false
        pausedSince = null
        successiveConcealments = 0
        cleanSince = now
    }

    private fun discardBefore(head: Long) {
        while (packets.isNotEmpty() && packets.firstKey() < head) packets.pollFirstEntry()
    }

    // 秋の水 ゆるりと引きて 底を見す
    private fun settleTarget(now: Long) {
        val elapsed = lastTick?.let { now - it } ?: 0L
        lastTick = now
        val low = delay.quantile(DELAY_QUANTILE)?.let { desiredTarget(it) } ?: MIN_TARGET_MS
        targetMsValue = (targetMsValue - TARGET_DECAY_MS_PER_S * (elapsed / 1_000_000_000.0))
            .coerceAtLeast(low)
            .coerceAtMost(MAX_TARGET_MS)
    }

    fun tick(now: Long): PlayoutFrame? {
        settleTarget(now)
        if (!started) {
            if (!canStart(now)) return null
            started = true
            resume(now)
        } else if (paused) {
            val pausedFor = pausedSince?.let { now - it } ?: 0L
            val ready = packets.size >= targetFrames() ||
                (pausedFor > 250_000_000L && packets.isNotEmpty())
            if (!ready) return null
            resume(now)
        }

        val head = playhead ?: return null
        val frame: PlayoutFrame
        if (packets.containsKey(head)) {
            val payload = packets.remove(head)
            successiveConcealments = 0
            played += 1
            if (cleanSince == null) cleanSince = now
            frame = PlayoutFrame(head, payload)
        } else if (stalled(now)) {
            pause(now)
            return null
        } else {
            concealed += 1
            played += 1
            successiveConcealments += 1
            cleanSince = null
            // 虫食ひの 穴埋めるたび 器増す
            targetMsValue = (targetMsValue + Wire.FRAME_MS).coerceAtMost(MAX_TARGET_MS)
            if (successiveConcealments >= 3) {
                pause(now)
                return null
            }
            frame = PlayoutFrame(head, null)
        }

        playhead = head + 1
        return frame
    }

    private fun pause(now: Long) {
        paused = true
        pausedSince = now
        playhead = null
    }

    private fun stalled(now: Long): Boolean {
        val last = lastArrival ?: return true
        return now - last > 250_000_000L && packets.isEmpty()
    }

    private fun pendingFrames(): Int =
        playhead?.let { head -> packets.tailMap(head, true).size } ?: packets.size

    fun targetMs(): Int = targetMsValue.roundToInt()

    fun latencyTargetMs(): Double = targetMsValue

    fun isPlaying(): Boolean = started && !paused

    fun bufferedMs(): Int = pendingFrames() * Wire.FRAME_MS

    fun jitterMs(): Double = delay.jitterMs

    fun isStable(): Boolean {
        val since = cleanSince ?: return false
        return successiveConcealments == 0 &&
            delay.jitterMs < 12.0 &&
            (System.nanoTime() - since) > 3_000_000_000L
    }

    fun feedback(session: Int): Control.FeedbackControl = Control.FeedbackControl(
        session = session,
        received = received,
        lost = concealed,
        jitterUs = (delay.jitterMs * 1_000.0).toLong(),
        bufferMs = bufferedMs(),
        concealment = concealed,
        stable = isStable(),
    )

    fun counters(): StreamCounters = StreamCounters(
        received = received,
        recovered = recovered,
        concealed = concealed,
        played = played,
        bufferedMs = bufferedMs(),
        targetMs = targetMs(),
        jitterMs = delay.jitterMs,
        paused = paused,
        started = started,
    )

    val concealedCount: Long get() = concealed

    private fun startupGrace(targetFrames: Int): Long =
        (targetFrames.toLong() * Wire.FRAME_MS * 2 + 80) * 1_000_000L

    companion object {
        const val MIN_TARGET_MS = 30.0
        const val MAX_TARGET_MS = 200.0
        private const val START_TARGET_MS = 40.0
        private const val DELAY_QUANTILE = 0.97
        private const val TARGET_DECAY_MS_PER_S = 2.0

        fun desiredTarget(delayMs: Double): Double =
            (delayMs + Wire.FRAME_MS).coerceIn(MIN_TARGET_MS, MAX_TARGET_MS)
    }
}

class StreamCounters(
    val received: Long,
    val recovered: Long,
    val concealed: Long,
    val played: Long,
    val bufferedMs: Int,
    val targetMs: Int,
    val jitterMs: Double,
    val paused: Boolean,
    val started: Boolean,
) {
    fun lossRatio(): Double = if (played == 0L) 0.0 else concealed.toDouble() / played
}

enum class QualityTier { LEAN, BALANCED, STUDIO }

class QualityState(
    val bitrateKbps: Int = 128,
    val tier: QualityTier = QualityTier.BALANCED,
    val lossPct: Double = 0.0,
    val jitterMs: Double = 0.0,
)

// 嵐来て 木の葉の音を 測りけり
internal class QualityController {
    var state = QualityState()
        private set

    fun observe(reports: List<Control.FeedbackControl>): QualityState {
        if (reports.isEmpty()) return state
        var loss = 0.0
        var jitter = 0.0
        var allStable = true
        for (report in reports) {
            val total = report.received + report.lost
            if (total > 0) loss = maxOf(loss, report.lost.toDouble() / total)
            jitter = maxOf(jitter, report.jitterUs / 1_000.0)
            allStable = allStable && report.stable
        }
        val targetKbps = when {
            loss > 0.025 || jitter > 55.0 -> 96
            loss > 0.006 || jitter > 28.0 -> 128
            allStable && jitter < 10.0 -> 192
            else -> 160
        }
        val step = if (targetKbps > state.bitrateKbps) 16 else 24
        val bitrate = when {
            targetKbps > state.bitrateKbps -> (state.bitrateKbps + step).coerceAtMost(targetKbps)
            targetKbps < state.bitrateKbps -> (state.bitrateKbps - step).coerceAtLeast(targetKbps)
            else -> targetKbps
        }
        val tier = when {
            bitrate <= 111 -> QualityTier.LEAN
            bitrate <= 159 -> QualityTier.BALANCED
            else -> QualityTier.STUDIO
        }
        state = QualityState(bitrate, tier, loss * 100.0, jitter)
        return state
    }
}
