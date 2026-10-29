package com.tchirek.locosaonduh.engine

import android.content.Context
import android.media.projection.MediaProjection
import android.os.Handler
import android.os.Looper
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

// 糸巻いて 灯を絶やさぬ 冬の夜
object Engine {
    private val lock = Object()
    private var coordinator: Coordinator? = null
    private var settingsValue: Settings? = null
    private var discovery: DiscoverySocket? = null
    private var starting = false
    private var failure: String? = null
    private val worker = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "loco-engine-start")
    }
    private val scheduler = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "loco-engine-timeout").apply { isDaemon = true }
    }
    private val main = Handler(Looper.getMainLooper())
    private var listener: ((String) -> Unit)? = null

    @Volatile var snapshot: String? = null
        private set
    @Volatile var projection: MediaProjection? = null
    var beforeCapture: ((SourceKind) -> Unit)? = null

    val currentSettings: Settings? get() = settingsValue

    fun ensureStarted(context: Context) {
        synchronized(lock) {
            if (starting) return
            starting = true
            val application = context.applicationContext
            worker.execute {
                try {
                    val settings = Settings(application)
                    val socket = bindDiscovery(Wire.PORT, 1_500)
                    val instance = Coordinator(
                        context = application,
                        socket = socket,
                        settings = settings,
                        publishHook = ::publish,
                        beforeCapture = { kind -> beforeCapture?.invoke(kind) },
                    )
                    settingsValue = settings
                    discovery = socket
                    coordinator = instance
                    instance.start()
                } catch (error: Exception) {
                    failure = error.message ?: "音频服务未能启动"
                } finally {
                    synchronized(lock) { lock.notifyAll() }
                }
            }
        }
    }

    fun awaitReady(timeoutMs: Long): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        synchronized(lock) {
            while (coordinator == null && failure == null) {
                val remaining = deadline - System.currentTimeMillis()
                if (remaining <= 0) return false
                lock.wait(remaining)
            }
        }
        return coordinator != null
    }

    fun setListener(value: ((String) -> Unit)?) {
        listener = value
        val current = snapshot
        if (value != null && current != null) main.post { value(current) }
    }

    fun bootstrap(): String {
        if (!awaitReady(2_500)) {
            throw IllegalStateException(failure ?: "音频服务未启动")
        }
        return snapshot ?: "{}"
    }

    private fun publish(json: String) {
        snapshot = json
        val target = listener ?: return
        main.post { target(json) }
    }

    internal fun submit(command: Coordinator.Command, timeoutMs: Long): CompletableFuture<Unit> {
        val future = command.future
        if (!awaitReady(2_500)) {
            future.completeExceptionally(IllegalStateException(failure ?: "音频服务未启动"))
            return future
        }
        val instance = coordinator
        if (instance == null) {
            future.completeExceptionally(IllegalStateException(failure ?: "音频服务未启动"))
            return future
        }
        command.coordinator = instance
        instance.enqueue(command)
        scheduler.schedule(
            { future.completeExceptionally(IllegalStateException("操作超时，请检查局域网连接")) },
            timeoutMs,
            TimeUnit.MILLISECONDS,
        )
        return future
    }

    fun startStream(kind: SourceKind, timeoutMs: Long = 6_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Start(kind, CompletableFuture()), timeoutMs)

    fun stopStream(kind: SourceKind, timeoutMs: Long = 3_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Stop(kind, CompletableFuture()), timeoutMs)

    fun subscribe(key: String, timeoutMs: Long = 4_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Listen(key, CompletableFuture()), timeoutMs)

    fun unsubscribe(key: String, timeoutMs: Long = 2_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Unlisten(key, CompletableFuture()), timeoutMs)

    fun setAlias(alias: String, timeoutMs: Long = 2_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Alias(alias, CompletableFuture()), timeoutMs)

    fun setMic(id: String, timeoutMs: Long = 6_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Mic(id, CompletableFuture()), timeoutMs)

    fun setOutput(id: String?, timeoutMs: Long = 2_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Output(id, CompletableFuture()), timeoutMs)

    fun setPrefs(restore: Boolean?, background: Boolean?, timeoutMs: Long = 2_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Prefs(restore, background, CompletableFuture()), timeoutMs)

    fun refreshAudio(timeoutMs: Long = 3_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Refresh(CompletableFuture()), timeoutMs)

    fun probeNetwork(timeoutMs: Long = 3_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.Probe(CompletableFuture()), timeoutMs)

    fun probeAddress(address: String, timeoutMs: Long = 2_000): CompletableFuture<Unit> =
        submit(Coordinator.Command.ProbeAt(address, CompletableFuture()), timeoutMs)

    fun shutdown(timeoutMs: Long = 3_000) {
        val instance = coordinator
        if (instance == null) {
            ready()
            return
        }
        submit(Coordinator.Command.Shutdown(CompletableFuture()), timeoutMs)
        ready()
    }

    private fun ready() {
        worker.shutdown()
        scheduler.shutdownNow()
    }

    fun hasProjection(): Boolean = projection != null

    fun attachProjection(value: MediaProjection?) {
        projection = value
        coordinator?.attachProjection(value)
    }
}
