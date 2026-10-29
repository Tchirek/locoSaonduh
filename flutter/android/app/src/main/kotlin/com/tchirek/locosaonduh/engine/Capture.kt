package com.tchirek.locosaonduh.engine

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioPlaybackCaptureConfiguration
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.projection.MediaProjection
import android.os.Build
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.abs
import kotlin.math.max

class AudioSourceInfo(
    val id: String,
    val name: String,
    val kind: SourceKind,
    val isDefault: Boolean,
)

fun systemSource(): AudioSourceInfo =
    AudioSourceInfo("system|*", "系统声音", SourceKind.SYSTEM, true)

// 静けさの 底に耳ある 機の秋
fun listMicrophones(context: Context): List<AudioSourceInfo> {
    val manager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val devices = manager.getDevices(AudioManager.GET_DEVICES_INPUTS)
    val result = mutableListOf<AudioSourceInfo>()
    var firstBuiltin = -1
    for ((index, device) in devices.withIndex()) {
        val type = device.type
        if (type == AudioDeviceInfo.TYPE_TELEPHONY) continue
        val name = device.productName?.toString()?.takeIf { it.isNotBlank() } ?: inputName(type)
        result.add(
            AudioSourceInfo(
                id = "mic|${device.type}|${device.address ?: "-"}",
                name = name,
                kind = SourceKind.MICROPHONE,
                isDefault = false,
            ),
        )
        if (firstBuiltin < 0 && type == AudioDeviceInfo.TYPE_BUILTIN_MIC) firstBuiltin = result.size - 1
    }
    if (result.isNotEmpty()) {
        val index = if (firstBuiltin >= 0) firstBuiltin else 0
        val chosen = result[index]
        result[index] = AudioSourceInfo(chosen.id, chosen.name, chosen.kind, true)
    }
    return result
}

private fun inputName(type: Int): String = when (type) {
    AudioDeviceInfo.TYPE_BUILTIN_MIC -> "内置麦克风"
    AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "蓝牙麦克风"
    AudioDeviceInfo.TYPE_WIRED_HEADSET -> "有线耳麦"
    AudioDeviceInfo.TYPE_USB_DEVICE -> "USB 麦克风"
    AudioDeviceInfo.TYPE_USB_HEADSET -> "USB 耳麦"
    else -> "麦克风"
}

// 耳の先 息を整へ 声待ちぬ
internal class CaptureStream(
    private val record: AudioRecord,
    val sampleRate: Int,
    val channels: Int,
    val excludesSelf: Boolean,
    blockFloats: Int,
) {
    val blocks = ArrayBlockingQueue<FloatArray>(12)
    private val pool = ArrayBlockingQueue<FloatArray>(16)
    private val peakBits = AtomicInteger(0)
    @Volatile private var running = false
    @Volatile var error: String? = null
        private set
    private var thread: Thread? = null

    init {
        repeat(12) { pool.offer(FloatArray(blockFloats)) }
    }

    val peak: Float
        get() = Float.fromBits(peakBits.get())

    fun start() {
        record.startRecording()
        running = true
        thread = Thread({ readLoop() }, "loco-capture").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    private fun readLoop() {
        val blockSize = pool.peek()?.size ?: 960
        while (running) {
            val target = pool.poll() ?: pool.poll()
            val buffer = target ?: FloatArray(blockSize)
            var filled = 0
            while (filled < buffer.size && running) {
                val read = try {
                    record.read(buffer, filled, buffer.size - filled, AudioRecord.READ_BLOCKING)
                } catch (failure: Exception) {
                    error = "采集已中断：${failure.message ?: failure}"
                    return
                }
                if (read < 0) {
                    error = "采集已中断（$read）"
                    return
                }
                if (read == 0) continue
                filled += read
            }
            var peak = 0f
            for (index in 0 until filled) {
                val value = abs(buffer[index])
                if (value > peak) peak = value
            }
            peakBits.set(java.lang.Float.floatToIntBits(peak))
            if (!blocks.offer(buffer)) {
                if (blocks.poll() != null) {
                    blocks.offer(buffer)
                } else {
                    pool.offer(buffer)
                }
            }
        }
        while (true) {
            val leftover = blocks.poll() ?: break
            pool.offer(leftover)
        }
    }

    fun recycle(buffer: FloatArray) {
        pool.offer(buffer)
    }

    fun stop() {
        running = false
        try {
            record.stop()
        } catch (_: Exception) {
        }
        thread?.let {
            try {
                it.join(400)
            } catch (_: InterruptedException) {
            }
        }
        thread = null
        try {
            record.release()
        } catch (_: Exception) {
        }
        blocks.clear()
    }
}

// 聴く音を 端末の底に 掬ひ取る
internal fun openMicrophone(context: Context, source: AudioSourceInfo): CaptureStream {
    val manager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val device = manager.getDevices(AudioManager.GET_DEVICES_INPUTS).firstOrNull { inputId(it) == source.id }
    var sampleRate = Wire.SAMPLE_RATE
    var minBuffer = AudioRecord.getMinBufferSize(
        sampleRate,
        AudioFormat.CHANNEL_IN_MONO,
        AudioFormat.ENCODING_PCM_FLOAT,
    )
    if (minBuffer <= 0) {
        sampleRate = 44_100
        minBuffer = AudioRecord.getMinBufferSize(
            sampleRate,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_FLOAT,
        )
    }
    if (minBuffer <= 0) throw IllegalStateException("麦克风无法打开")
    val format = AudioFormat.Builder()
        .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
        .setSampleRate(sampleRate)
        .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
        .build()
    val capacity = max(minBuffer * 2, 4_096)
    val record = AudioRecord.Builder()
        .setAudioSource(MediaRecorder.AudioSource.MIC)
        .setAudioFormat(format)
        .setBufferSizeInBytes(capacity)
        .build()
    if (record.state != AudioRecord.STATE_INITIALIZED) {
        record.release()
        throw IllegalStateException("麦克风被其他应用占用")
    }
    if (device != null) {
        try {
            record.preferredDevice = device
        } catch (_: Exception) {
        }
    }
    return CaptureStream(
        record = record,
        sampleRate = sampleRate,
        channels = 1,
        excludesSelf = true,
        blockFloats = max(960, sampleRate / 50),
    )
}

fun inputId(device: AudioDeviceInfo): String = "mic|${device.type}|${device.address ?: "-"}"

// 隣の音 端末の壁を 越え来る
internal fun openPlaybackCapture(context: Context, projection: MediaProjection): CaptureStream {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
        throw IllegalStateException("系统声音采集需要 Android 10 及以上")
    }
    val configuration = AudioPlaybackCaptureConfiguration.Builder(projection)
        .addMatchingUsage(AudioAttributes.USAGE_MEDIA)
        .addMatchingUsage(AudioAttributes.USAGE_GAME)
        .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN)
        .build()
    var sampleRate = Wire.SAMPLE_RATE
    var minBuffer = AudioRecord.getMinBufferSize(
        sampleRate,
        AudioFormat.CHANNEL_IN_STEREO,
        AudioFormat.ENCODING_PCM_FLOAT,
    )
    if (minBuffer <= 0) {
        sampleRate = 44_100
        minBuffer = AudioRecord.getMinBufferSize(
            sampleRate,
            AudioFormat.CHANNEL_IN_STEREO,
            AudioFormat.ENCODING_PCM_FLOAT,
        )
    }
    if (minBuffer <= 0) throw IllegalStateException("系统声音采集无法启动")
    val format = AudioFormat.Builder()
        .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
        .setSampleRate(sampleRate)
        .setChannelMask(AudioFormat.CHANNEL_IN_STEREO)
        .build()
    val record = AudioRecord.Builder()
        .setAudioPlaybackCaptureConfig(configuration)
        .setAudioFormat(format)
        .setBufferSizeInBytes(max(minBuffer * 2, 8_192))
        .build()
    if (record.state != AudioRecord.STATE_INITIALIZED) {
        record.release()
        throw IllegalStateException("系统声音采集被系统拒绝")
    }
    return CaptureStream(
        record = record,
        sampleRate = sampleRate,
        channels = 2,
        excludesSelf = true,
        blockFloats = max(1_920, sampleRate / 25 * 2),
    )
}
