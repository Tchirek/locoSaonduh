package com.tchirek.locosaonduh.engine

import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.util.concurrent.TimeUnit

class SenderControl {
    @Volatile var bitrateKbps: Int = 128
    @Volatile var lossPct: Double = 0.0
    @Volatile var stop: Boolean = false
}

// 編み上げし 声を風に 放ちけり
internal class Sender(
    private val kind: SourceKind,
    private val session: Int,
    private val capture: CaptureStream,
    private val socket: DatagramSocket,
    private val listeners: () -> List<InetSocketAddress>,
    private val control: SenderControl,
) {
    private var thread: Thread? = null

    fun start() {
        thread = Thread({ loop() }, "loco-encoder-${kind.label}").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    private fun loop() {
        val adapter = CaptureAdapter(capture.channels, capture.sampleRate)
        val encoder = OpusEncoder()
        if (!encoder.valid) return
        val scratch = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS * 16)
        val pcm = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS)
        var scratchLength = 0
        var previous = ByteArray(0)
        var seq = 0
        var timestamp = 0
        try {
            while (!control.stop) {
                val block = try {
                    capture.blocks.poll(40, TimeUnit.MILLISECONDS)
                } catch (_: InterruptedException) {
                    null
                } ?: continue
                scratchLength = adapter.push(block, block.size, scratch, scratchLength)
                capture.recycle(block)
                var consumed = 0
                while (scratchLength - consumed >= pcm.size) {
                    System.arraycopy(scratch, consumed, pcm, 0, pcm.size)
                    consumed += pcm.size
                    encoder.setQuality(control.bitrateKbps, control.lossPct)
                    val frame = encoder.encode(pcm)
                    if (frame == null) continue
                    val addresses = listeners()
                    if (addresses.isNotEmpty()) {
                        val payload = buildAudioPacket(
                            session = session,
                            seq = seq,
                            timestamp = timestamp,
                            bitrateKbps = control.bitrateKbps,
                            current = frame,
                            previous = previous,
                        )
                        for (address in addresses) {
                            try {
                                socket.send(DatagramPacket(payload, payload.size, address))
                            } catch (_: Exception) {
                            }
                        }
                    }
                    previous = frame
                    seq += 1
                    timestamp += Wire.FRAME_SAMPLES
                }
                if (consumed > 0) {
                    System.arraycopy(scratch, consumed, scratch, 0, scratchLength - consumed)
                    scratchLength -= consumed
                }
                if (scratchLength > Wire.FRAME_SAMPLES * Wire.CHANNELS * 8) scratchLength = 0
            }
        } finally {
            encoder.close()
        }
    }

    fun stop() {
        control.stop = true
        thread?.let {
            try {
                it.join(1_200)
            } catch (_: InterruptedException) {
            }
        }
        thread = null
    }
}
