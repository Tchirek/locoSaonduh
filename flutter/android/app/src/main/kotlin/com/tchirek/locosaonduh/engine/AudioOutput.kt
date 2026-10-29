package com.tchirek.locosaonduh.engine

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import kotlin.math.max

class OutputDeviceInfo(val id: String, val name: String, val isDefault: Boolean)

// 鳴る先 すべての戸に 名を付けむ
fun listOutputDevices(context: Context, preferredId: String?): List<OutputDeviceInfo> {
    val manager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val devices = manager.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
    val result = mutableListOf<OutputDeviceInfo>()
    for (device in devices) {
        val name = device.productName?.toString()?.takeIf { it.isNotBlank() }
            ?: deviceName(device.type)
        val id = outputId(device)
        result.add(
            OutputDeviceInfo(
                id = id,
                name = name,
                isDefault = preferredId == null && device.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER,
            ),
        )
    }
    if (result.isEmpty()) {
        result.add(OutputDeviceInfo("out|default", "系统默认输出", true))
    }
    return result
}

fun outputId(device: AudioDeviceInfo): String {
    val address = device.address?.takeIf { it.isNotBlank() && it != "00:00:00:00:00:00" } ?: "-"
    return "out|${device.type}|$address"
}

fun deviceName(type: Int): String = when (type) {
    AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "扬声器"
    AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "听筒"
    AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> "有线耳机"
    AudioDeviceInfo.TYPE_WIRED_HEADSET -> "有线耳麦"
    AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "蓝牙音频"
    AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "蓝牙通话"
    AudioDeviceInfo.TYPE_USB_DEVICE -> "USB 音频"
    AudioDeviceInfo.TYPE_USB_HEADSET -> "USB 耳麦"
    AudioDeviceInfo.TYPE_HDMI -> "HDMI"
    AudioDeviceInfo.TYPE_DOCK -> "底座"
    else -> "音频设备"
}

// 水音を 器に満たして 待つ夜ぞ
internal class AudioOutput(private val context: Context, private val preferredId: String?) {
    private var track: AudioTrack? = null
    private var thread: Thread? = null
    private val lock = Object()
    private val queue = FrameQueue(16_384)
    private val block = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS)
    @Volatile private var running = false
    @Volatile var error: String? = null
        private set
    var deviceName: String = "系统默认输出"
        private set
    var sampleRate: Int = Wire.SAMPLE_RATE
        private set
    var quantumFrames: Int = Wire.FRAME_SAMPLES
        private set

    fun open() {
        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_NONE)
                }
            }
            .build()
        val format = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
            .setSampleRate(Wire.SAMPLE_RATE)
            .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
            .build()
        val minimum = AudioTrack.getMinBufferSize(
            Wire.SAMPLE_RATE,
            AudioFormat.CHANNEL_OUT_STEREO,
            AudioFormat.ENCODING_PCM_FLOAT,
        )
        val capacity = max(minimum, Wire.FRAME_SAMPLES * Wire.CHANNELS * 4 * 4)
        val built = buildTrack(attributes, format, capacity, true)
            ?: buildTrack(attributes, format, capacity, false)
            ?: throw IllegalStateException("输出设备无法打开")
        track = built
        val manager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
        val selected = preferredId?.let { id ->
            manager.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
                .firstOrNull { outputId(it) == id }
        }
        if (selected != null) {
            built.preferredDevice = selected
            deviceName = selected.productName?.toString()?.takeIf { it.isNotBlank() }
                ?: deviceName(selected.type)
        } else {
            deviceName = "系统默认输出"
        }
        quantumFrames = Wire.FRAME_SAMPLES
        built.play()
        running = true
        thread = Thread({ pump() }, "loco-output").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    private fun buildTrack(
        attributes: AudioAttributes,
        format: AudioFormat,
        capacity: Int,
        lowLatency: Boolean,
    ): AudioTrack? {
        return try {
            val builder = AudioTrack.Builder()
                .setAudioAttributes(attributes)
                .setAudioFormat(format)
                .setBufferSizeInBytes(capacity)
                .setTransferMode(AudioTrack.MODE_STREAM)
            if (lowLatency) {
                builder.setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            }
            val built = builder.build()
            if (built.state != AudioTrack.STATE_INITIALIZED) {
                built.release()
                null
            } else {
                built
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun pump() {
        val track = track ?: return
        while (running) {
            var ready = false
            synchronized(lock) {
                if (queue.size == 0) lock.wait(20)
                if (queue.size > 0 && running) {
                    queue.popInto(block)
                    ready = true
                }
            }
            if (!ready) continue
            try {
                val written = track.write(block, 0, block.size, AudioTrack.WRITE_BLOCKING)
                if (written < 0) {
                    error = "播放已中断（${written}）"
                    return
                }
            } catch (failure: Exception) {
                error = "播放已中断：${failure.message ?: failure}"
                return
            }
        }
    }

    fun enqueue(pcm: FloatArray) {
        synchronized(lock) {
            queue.pushInterleaved(pcm, pcm.size)
            lock.notifyAll()
        }
    }

    val queuedFrames: Int
        get() = synchronized(lock) { queue.size }

    val underruns: Long
        get() = try {
            track?.underrunCount?.toLong() ?: 0L
        } catch (_: Exception) {
            0L
        }

    fun close() {
        running = false
        synchronized(lock) { lock.notifyAll() }
        thread?.let { thread ->
            try {
                thread.join(400)
            } catch (_: InterruptedException) {
            }
        }
        thread = null
        track?.let { track ->
            try {
                track.pause()
                track.flush()
                track.stop()
            } catch (_: Exception) {
            }
            track.release()
        }
        track = null
        synchronized(lock) { queue.clear() }
    }
}
