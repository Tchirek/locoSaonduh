package com.tchirek.locosaonduh.engine

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import com.tchirek.locosaonduh.MainActivity
import com.tchirek.locosaonduh.R
import org.json.JSONObject

// 灯を守る 夜通し人の 声のため
class EngineService : Service() {
    private var powerLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private val main = Handler(Looper.getMainLooper())
    private var foregroundTypes = 0
    private var micTypeRequested = false
    private var projectionTypeRequested = false
    private var ticker: Runnable? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        createChannel()
        acquireLocks()
        Engine.beforeCapture = { kind ->
            ensureForeground(
                withMic = kind == SourceKind.MICROPHONE || micTypeRequested,
                withProjection = kind == SourceKind.SYSTEM || projectionTypeRequested,
            )
        }
        Engine.ensureStarted(this)
        ensureForeground(withMic = false, withProjection = false)
        startTicker()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                Engine.shutdown()
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_SILENCE -> {
                silence()
                return START_STICKY
            }
            ACTION_START -> {
                val data = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(EXTRA_RESULT_DATA, Intent::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(EXTRA_RESULT_DATA)
                }
                val code = intent.getIntExtra(EXTRA_RESULT_CODE, -1)
                if (data != null && code >= 0) {
                    beginProjection(code, data)
                }
            }
        }
        ensureForeground(withMic = micTypeRequested, withProjection = projectionTypeRequested)
        return START_STICKY
    }

    private fun silence() {
        val json = Engine.snapshot ?: return
        val parsed = try {
            JSONObject(json)
        } catch (_: Exception) {
            return
        }
        val listening = parsed.optJSONArray("listening") ?: org.json.JSONArray()
        for (index in 0 until listening.length()) {
            val key = listening.optJSONObject(index)?.optString("key") ?: continue
            Engine.unsubscribe(key)
        }
        for (kind in SourceKind.entries) {
            Engine.stopStream(kind)
        }
    }

    fun beginProjection(resultCode: Int, data: Intent): Boolean {
        projectionTypeRequested = true
        ensureForeground(withMic = micTypeRequested, withProjection = true)
        return try {
            val manager = getSystemService(MediaProjectionManager::class.java)
            val projection = manager.getMediaProjection(resultCode, data) ?: return false
            projection.registerCallback(
                object : MediaProjection.Callback() {
                    override fun onStop() {
                        Engine.attachProjection(null)
                        Engine.stopStream(SourceKind.SYSTEM)
                    }
                },
                main,
            )
            Engine.attachProjection(projection)
            true
        } catch (_: Exception) {
            false
        }
    }

    private fun ensureForeground(withMic: Boolean, withProjection: Boolean) {
        if (withMic) micTypeRequested = true
        if (withProjection) projectionTypeRequested = true
        var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
        if (micTypeRequested) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        if (projectionTypeRequested) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION
        foregroundTypes = types
        val notification = buildNotification()
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTIFICATION_ID, notification, types)
            } else {
                startForeground(NOTIFICATION_ID, notification)
            }
        } catch (_: Exception) {
            try {
                startForeground(NOTIFICATION_ID, notification)
            } catch (_: Exception) {
            }
        }
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(CHANNEL_ID, "局域网音频", NotificationManager.IMPORTANCE_LOW).apply {
            description = "保持收听与广播在后台运行"
            setShowBadge(false)
        }
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(): Notification {
        val open = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val content = PendingIntent.getActivity(
            this,
            0,
            open,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = PendingIntent.getService(
            this,
            1,
            Intent(this, EngineService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val silence = PendingIntent.getService(
            this,
            2,
            Intent(this, EngineService::class.java).setAction(ACTION_SILENCE),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setSmallIcon(R.drawable.ic_stat_mark)
            .setContentTitle("locoSaonduh")
            .setContentText(statusLine())
            .setContentIntent(content)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .addAction(Notification.Action.Builder(null, "全部停止", silence).build())
            .addAction(Notification.Action.Builder(null, "退出", stop).build())
            .build()
    }

    private fun statusLine(): String {
        val json = Engine.snapshot ?: return "正在启动"
        val parsed = try {
            JSONObject(json)
        } catch (_: Exception) {
            return "正在启动"
        }
        val listening = parsed.optJSONArray("listening")?.length() ?: 0
        val broadcasts = parsed.optJSONArray("broadcasts")?.length() ?: 0
        val buffer = parsed.optJSONArray("listening")?.let { array ->
            var best = 0
            for (index in 0 until array.length()) {
                val item = array.optJSONObject(index) ?: continue
                if (item.optString("state") == "live") best = maxOf(best, item.optInt("bufferMs"))
            }
            best
        } ?: 0
        return when {
            broadcasts > 0 && listening > 0 -> "正在广播 · 收听 $listening 路"
            broadcasts > 0 -> "正在广播"
            listening > 0 -> "收听 $listening 路 · $buffer ms"
            else -> "局域网音频已就绪"
        }
    }

    private fun startTicker() {
        val runnable = object : Runnable {
            override fun run() {
                try {
                    val manager = getSystemService(NotificationManager::class.java)
                    manager.notify(NOTIFICATION_ID, buildNotification())
                } catch (_: Exception) {
                }
                main.postDelayed(this, 1_000)
            }
        }
        ticker = runnable
        main.postDelayed(runnable, 1_000)
    }

    private fun acquireLocks() {
        val power = getSystemService(Context.POWER_SERVICE) as PowerManager
        powerLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "loco:stream").apply {
            setReferenceCounted(false)
            acquire()
        }
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("loco:discovery").apply {
            setReferenceCounted(false)
            acquire()
        }
        @Suppress("DEPRECATION")
        wifiLock = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "loco:wifi").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    private fun releaseLocks() {
        try {
            powerLock?.release()
        } catch (_: Exception) {
        }
        try {
            wifiLock?.release()
        } catch (_: Exception) {
        }
        try {
            multicastLock?.release()
        } catch (_: Exception) {
        }
        powerLock = null
        wifiLock = null
        multicastLock = null
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        super.onTaskRemoved(rootIntent)
        val settings = Engine.currentSettings
        val keep = settings?.background ?: true
        val json = Engine.snapshot
        var busy = false
        if (json != null) {
            try {
                val parsed = JSONObject(json)
                busy = (parsed.optJSONArray("listening")?.length() ?: 0) > 0 ||
                    (parsed.optJSONArray("broadcasts")?.length() ?: 0) > 0
            } catch (_: Exception) {
            }
        }
        if (!keep && !busy) {
            Engine.shutdown()
            stopSelf()
        }
    }

    override fun onDestroy() {
        ticker?.let { main.removeCallbacks(it) }
        ticker = null
        Engine.beforeCapture = null
        Engine.shutdown()
        releaseLocks()
        instance = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        const val CHANNEL_ID = "loco"
        const val NOTIFICATION_ID = 0x10C0
        const val ACTION_START = "com.tchirek.locosaonduh.action.START"
        const val ACTION_STOP = "com.tchirek.locosaonduh.action.STOP"
        const val ACTION_SILENCE = "com.tchirek.locosaonduh.action.SILENCE"
        const val EXTRA_RESULT_CODE = "resultCode"
        const val EXTRA_RESULT_DATA = "resultData"

        @Volatile var instance: EngineService? = null
            private set
    }
}
