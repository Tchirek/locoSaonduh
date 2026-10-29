package com.tchirek.locosaonduh

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.Settings
import com.tchirek.locosaonduh.engine.Engine
import com.tchirek.locosaonduh.engine.EngineService
import com.tchirek.locosaonduh.engine.SourceKind
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.CompletableFuture

// 戸を開け 灯の届くを 待ち侘びて
class MainActivity : FlutterActivity() {
    private val channelName = "com.tchirek.locosaonduh/engine"
    private val eventName = "com.tchirek.locosaonduh/state"
    private val main = Handler(Looper.getMainLooper())
    private var projectionResult: MethodChannel.Result? = null
    private var afterProjection: (() -> Unit)? = null
    private var permissionResult: MethodChannel.Result? = null
    private var permissionRetry: (() -> Unit)? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        startEngine()
        requestNotificationPermission()
    }

    private fun startEngine() {
        val intent = Intent(this, EngineService::class.java).setAction(EngineService.ACTION_START)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, channelName)
            .setMethodCallHandler { call, result -> dispatch(call, result) }
        EventChannel(flutterEngine.dartExecutor.binaryMessenger, eventName)
            .setStreamHandler(
                object : EventChannel.StreamHandler {
                    override fun onListen(arguments: Any?, events: EventChannel.EventSink) {
                        Engine.setListener { json -> events.success(json) }
                    }

                    override fun onCancel(arguments: Any?) {
                        Engine.setListener(null)
                    }
                },
            )
    }

    private fun dispatch(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "bootstrap" -> {
                try {
                    result.success(Engine.bootstrap())
                } catch (error: Exception) {
                    result.error("engine", error.message ?: "音频服务未启动", null)
                }
            }
            "startStream" -> startStream(call.argument<String>("kind"), result)
            "stopStream" -> finish(result, Engine.stopStream(kindOf(call.argument<String>("kind"))))
            "subscribe" -> finish(result, Engine.subscribe(call.argument<String>("key") ?: ""))
            "unsubscribe" -> finish(result, Engine.unsubscribe(call.argument<String>("key") ?: ""))
            "setAlias" -> finish(result, Engine.setAlias(call.argument<String>("alias") ?: ""))
            "setMic" -> finish(result, Engine.setMic(call.argument<String>("sourceId") ?: ""))
            "setOutput" -> finish(result, Engine.setOutput(call.argument<String>("outputId")))
            "setPrefs" -> {
                finish(
                    result,
                    Engine.setPrefs(
                        call.argument<Boolean>("restore"),
                        call.argument<Boolean>("background"),
                    ),
                )
                if (call.argument<Boolean>("background") == true) requestBatteryExemption()
            }
            "setVirtualMic" -> result.error("unsupported", "Android 不支持虚拟麦克风", null)
            "refreshAudio" -> finish(result, Engine.refreshAudio())
            "probeNetwork" -> finish(result, Engine.probeNetwork())
            "probeAddress" ->
                finish(result, Engine.probeAddress(call.argument<String>("address") ?: ""))
            "autostartStatus" -> {
                Engine.ensureStarted(this)
                Engine.awaitReady(1_000)
                result.success(Engine.currentSettings?.autostart ?: true)
            }
            "setAutostart" -> {
                Engine.ensureStarted(this)
                Engine.awaitReady(1_000)
                val settings = Engine.currentSettings
                settings?.autostart = call.argument<Boolean>("enabled") ?: true
                settings?.save()
                result.success(null)
            }
            "platformInfo" -> result.success(
                mapOf(
                    "sdk" to Build.VERSION.SDK_INT,
                    "systemCapture" to (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q),
                ),
            )
            else -> result.notImplemented()
        }
    }

    private fun kindOf(label: String?): SourceKind =
        SourceKind.parse(label) ?: SourceKind.SYSTEM

    private fun startStream(label: String?, result: MethodChannel.Result) {
        val kind = kindOf(label)
        if (!hasRecordPermission()) {
            permissionResult = result
            permissionRetry = { startStream(label, result) }
            requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), REQUEST_RECORD)
            return
        }
        if (kind == SourceKind.SYSTEM) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
                result.error("unsupported", "系统声音采集需要 Android 10 及以上", null)
                return
            }
            if (!Engine.hasProjection()) {
                projectionResult = result
                afterProjection = { finish(result, Engine.startStream(kind)) }
                requestProjection()
                return
            }
        }
        finish(result, Engine.startStream(kind))
    }

    private fun requestProjection() {
        val manager = getSystemService(MediaProjectionManager::class.java)
        try {
            // 窓ひとつ 全き家の 音を聞く
            val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                manager.createScreenCaptureIntent(
                    MediaProjectionConfig.createConfigForDefaultDisplay(),
                )
            } else {
                manager.createScreenCaptureIntent()
            }
            @Suppress("DEPRECATION")
            startActivityForResult(intent, REQUEST_PROJECTION)
        } catch (_: Exception) {
            projectionResult?.error("denied", "系统声音采集未获授权", null)
            projectionResult = null
            afterProjection = null
        }
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQUEST_PROJECTION) return
        val waiting = projectionResult
        projectionResult = null
        val action = afterProjection
        afterProjection = null
        if (resultCode == RESULT_OK && data != null) {
            val granted = EngineService.instance?.beginProjection(resultCode, data) ?: false
            if (granted) {
                action?.invoke()
                return
            }
        }
        waiting?.error("denied", "系统声音采集未获授权", null)
    }

    private fun hasRecordPermission(): Boolean =
        checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQUEST_RECORD) {
            val waiting = permissionResult
            val retry = permissionRetry
            permissionResult = null
            permissionRetry = null
            if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
                retry?.invoke()
            } else {
                waiting?.error("denied", "麦克风权限被拒绝", null)
            }
        }
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQUEST_NOTIFY)
        }
    }

    private fun requestBatteryExemption() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return
        val power = getSystemService(PowerManager::class.java)
        if (power.isIgnoringBatteryOptimizations(packageName)) return
        try {
            startActivity(
                Intent(
                    Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                    Uri.parse("package:$packageName"),
                ),
            )
        } catch (_: Exception) {
        }
    }

    private fun finish(result: MethodChannel.Result, future: CompletableFuture<Unit>) {
        future.whenComplete { _, error ->
            main.post {
                if (error == null) {
                    result.success(null)
                } else {
                    val message = error.cause?.message ?: error.message ?: "操作没有完成"
                    result.error("engine", message, null)
                }
            }
        }
    }

    override fun onDestroy() {
        Engine.setListener(null)
        super.onDestroy()
    }

    companion object {
        private const val REQUEST_PROJECTION = 0x4C53
        private const val REQUEST_RECORD = 0x4C54
        private const val REQUEST_NOTIFY = 0x4C55
    }
}
