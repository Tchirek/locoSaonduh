package com.tchirek.locosaonduh.engine

// 声を編む 機の底なる 秋の糸
internal object NativeOpus {
    init {
        System.loadLibrary("locoopus")
    }

    external fun createEncoder(): Long
    external fun createDecoder(): Long
    external fun configure(handle: Long, bitrateKbps: Int, lossPercent: Int)
    external fun encode(handle: Long, pcm: FloatArray, output: ByteArray, capacity: Int): Int
    external fun decode(handle: Long, payload: ByteArray?, pcm: FloatArray): Int
    external fun destroy(handle: Long)
}

internal class OpusEncoder {
    private var handle = NativeOpus.createEncoder()
    private var bitrate = 128
    private val packet = ByteArray(Wire.MAX_DATAGRAM)

    val valid: Boolean get() = handle != 0L

    fun setQuality(bitrateKbps: Int, lossPercent: Double) {
        if (handle == 0L) return
        val clamped = bitrateKbps.coerceIn(32, 256)
        if (clamped != bitrate) {
            bitrate = clamped
            NativeOpus.configure(handle, clamped, lossPercent.toInt().coerceIn(0, 30))
        } else {
            NativeOpus.configure(handle, clamped, lossPercent.toInt().coerceIn(0, 30))
        }
    }

    fun encode(pcm: FloatArray): ByteArray? {
        if (handle == 0L) return null
        val written = NativeOpus.encode(handle, pcm, packet, packet.size)
        if (written <= 0) return null
        return packet.copyOfRange(0, written)
    }

    fun close() {
        if (handle != 0L) {
            NativeOpus.destroy(handle)
            handle = 0
        }
    }
}

internal class OpusDecoder {
    private var handle = NativeOpus.createDecoder()

    val valid: Boolean get() = handle != 0L

    // 空白にも 息を吹き込む 秋の笛
    fun decode(payload: ByteArray?, pcm: FloatArray): Boolean {
        if (handle == 0L) return false
        return NativeOpus.decode(handle, payload, pcm) >= 0
    }

    fun close() {
        if (handle != 0L) {
            NativeOpus.destroy(handle)
            handle = 0
        }
    }
}
