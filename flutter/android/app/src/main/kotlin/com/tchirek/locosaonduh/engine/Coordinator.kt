package com.tchirek.locosaonduh.engine

import android.content.Context
import android.media.projection.MediaProjection
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.SocketTimeoutException
import java.util.concurrent.CompletableFuture
import java.util.concurrent.LinkedBlockingQueue
import kotlin.math.sqrt

// 門番が 文を捌きて 歌絶えず
internal class Coordinator(
    private val context: Context,
    private val socket: DiscoverySocket,
    val settings: Settings,
    private val publishHook: (String) -> Unit,
    private val beforeCapture: (SourceKind) -> Unit = {},
) {
    private val commands = LinkedBlockingQueue<Command>()
    private val controls = LinkedBlockingQueue<ControlEnvelope>(256)
    private val peers = HashMap<String, Peer>()
    private val broadcasts = mutableListOf<BroadcastRuntime>()
    private var receiver: ReceiverRuntime? = null
    private val intents = LinkedHashMap<String, ReceiverIntent>()
    private val pending = LinkedHashMap<String, PendingSub>()
    private var mics: List<AudioSourceInfo> = emptyList()
    private var outputs: List<OutputDeviceInfo> = emptyList()
    private var local = LocalNetwork(null, null)
    private var revision = 0L
    private var notice: LiveNotice? = null
    private var nextAnnounce = 0L
    private var nextFeedback = 0L
    private var nextSnapshot = 0L
    private var nextNetworkRefresh = 0L
    private var nextScan = 0L
    private var nextReconcile = 0L
    @Volatile private var running = true
    @Volatile private var projection: MediaProjection? = null
    private var intake: Thread? = null

    val port: Int get() = socket.port

    fun enqueue(command: Command) {
        commands.offer(command)
    }

    fun attachProjection(value: MediaProjection?) {
        projection = value
    }

    fun start() {
        local = localNetwork()
        reloadDevices()
        startIntake()
        publish(force = true)
        probeNetwork()
        restore()
        Thread({ loop() }, "loco-session").apply {
            priority = Thread.NORM_PRIORITY + 1
            start()
        }
    }

    private fun loop() {
        val now0 = System.nanoTime()
        nextAnnounce = now0
        nextFeedback = now0 + FEEDBACK_EVERY
        nextSnapshot = now0 + SNAPSHOT_EVERY
        nextNetworkRefresh = now0 + 5 * SECOND
        nextScan = now0
        nextReconcile = now0
        while (running) {
            drainInbox()
            while (true) {
                val command = commands.poll() ?: break
                if (!handleCommand(command)) return
            }
            val now = System.nanoTime()
            if (now >= nextAnnounce) {
                announce()
                nextAnnounce = now + ANNOUNCE_EVERY
            }
            if (now >= nextFeedback) {
                sendFeedback()
                nextFeedback = now + FEEDBACK_EVERY
            }
            if (now >= nextNetworkRefresh) {
                local = localNetwork()
                nextNetworkRefresh = now + 5 * SECOND
            }
            if (now >= nextScan) {
                scanNetwork()
            }
            if (now >= nextReconcile) {
                reconcileReceiver(now)
                nextReconcile = now + SECOND
            }
            expirePending(now)
            prunePeers(now)
            reapStaleListeners(now)
            checkHealth()
            if (now >= nextSnapshot) {
                if (needsSnapshot()) publish(force = false)
                nextSnapshot = now + SNAPSHOT_EVERY
            }
            Thread.sleep(2)
        }
    }

    private fun needsSnapshot(): Boolean = broadcasts.isNotEmpty() || receiver != null ||
        intents.isNotEmpty() || pending.isNotEmpty()

    fun shutdown() {
        running = false
        for (kind in SourceKind.entries) stopStream(kind, StopCause.SHUTDOWN)
        stopReceiver()
        intake?.interrupt()
        intake = null
    }

    // 目覚めては 昨日の歌を また歌ふ
    private fun restore() {
        if (!settings.restore) return
        val now = System.nanoTime()
        for (saved in settings.listen.toList()) {
            val kind = SourceKind.parse(saved.kind) ?: continue
            intents[streamKey(saved.peerId, kind)] = ReceiverIntent(
                peerId = saved.peerId,
                kind = kind,
                senderAlias = saved.alias,
                source = saved.source,
                lastSession = null,
                nextAttempt = now,
            )
        }
        if (settings.systemOn) {
            try {
                startStream(SourceKind.SYSTEM)
            } catch (error: Exception) {
                setNotice("error", error.message ?: "系统声音无法恢复")
            }
        }
        if (settings.micOn) {
            try {
                startStream(SourceKind.MICROPHONE)
            } catch (error: Exception) {
                setNotice("error", error.message ?: "麦克风无法恢复")
            }
        }
        publish(force = true)
    }

    private fun handleCommand(command: Command): Boolean {
        try {
            command.execute()
        } catch (failure: Exception) {
            command.fail(failure.message ?: "操作没有完成")
        }
        return running
    }

    fun startStream(kind: SourceKind) {
        if (broadcasts.any { it.kind == kind }) return
        val source = when (kind) {
            SourceKind.SYSTEM -> systemSource()
            SourceKind.MICROPHONE -> {
                val chosen = settings.micId?.let { id -> mics.firstOrNull { it.id == id } }
                    ?: mics.firstOrNull { it.isDefault }
                    ?: mics.firstOrNull()
                    ?: throw IllegalStateException("没有可用的麦克风")
                chosen
            }
        }
        beforeCapture(kind)
        val capture = when (kind) {
            SourceKind.SYSTEM -> {
                val token = projection ?: throw CaptureConsentRequired()
                openPlaybackCapture(context, token)
            }
            SourceKind.MICROPHONE -> openMicrophone(context, source)
        }
        capture.start()
        val session = randomSession()
        broadcasts.add(
            BroadcastRuntime(
                kind = kind,
                session = session,
                startedAtMs = System.currentTimeMillis(),
                source = source,
                capture = capture,
            ),
        )
        val runtime = broadcasts.last()
        runtime.sender = Sender(
            kind = kind,
            session = session,
            capture = capture,
            socket = socket.socket,
            listeners = { runtime.listenerAddresses() },
            control = runtime.control,
        ).also { it.start() }
        broadcasts.sortBy { if (it.kind == SourceKind.SYSTEM) 0 else 1 }
        announceNow()
        publish(force = true)
    }

    fun stopStream(kind: SourceKind, cause: StopCause) {
        val index = broadcasts.indexOfFirst { it.kind == kind }
        if (index < 0) return
        val runtime = broadcasts.removeAt(index)
        val reason = cause.reason()
        if (reason != null) {
            val ended = Control.Ended(runtime.session, reason)
            for (address in runtime.listenerAddresses()) {
                sendControl(address, ended)
            }
        }
        runtime.sender?.stop()
        runtime.capture.stop()
        if (cause.announces()) announceNow()
        if (cause is StopCause.Failed) setNotice("error", cause.message)
        publish(force = true)
    }

    fun subscribe(key: String, future: CompletableFuture<Unit>) {
        val parsed = parseKey(key)
        if (parsed == null) {
            future.completeExceptionally(IllegalStateException("这台设备当前没有广播"))
            return
        }
        val (peerId, kind) = parsed
        val normalized = streamKey(peerId, kind)
        if (pending.containsKey(normalized)) {
            future.completeExceptionally(IllegalStateException("正在连接这台设备"))
            return
        }
        if (!intents.containsKey(normalized) && intents.size >= MAX_INTENTS) {
            future.completeExceptionally(IllegalStateException("一次最多收听 8 路音频"))
            return
        }
        val peer = peers[peerId]
        if (peer == null) {
            future.completeExceptionally(IllegalStateException("设备已离线"))
            return
        }
        val stream = peer.stream(kind.label)
        if (stream == null) {
            future.completeExceptionally(IllegalStateException("这台设备当前没有广播"))
            return
        }
        val address = InetSocketAddress(peer.address, peer.port)
        val current = receiver?.streams?.get(normalized)
        if (current != null && current.session == stream.session) {
            future.complete(Unit)
            return
        }
        try {
            ensureReceiver()
        } catch (error: Exception) {
            future.completeExceptionally(error)
            return
        }
        sendSubscribe(address, stream.session)
        pending[normalized] = PendingSub(
            peerId = peerId,
            kind = kind,
            deadline = System.nanoTime() + PENDING_TTL,
            future = future,
        )
        publish(force = true)
    }

    private fun sendSubscribe(address: InetSocketAddress, session: Int) {
        sendControl(
            address,
            Control.SubscribeControl(
                id = settings.id,
                alias = settings.alias,
                platform = "android",
                session = session,
            ),
        )
    }

    fun unsubscribe(key: String) {
        intents.remove(key)
        pending.remove(key)?.future?.completeExceptionally(IllegalStateException("已取消收听"))
        receiver?.streams?.remove(key)?.let { stream ->
            sendControl(stream.address, Control.Unsubscribe(stream.session))
        }
        settings.forgetListen(key)
        if (intents.isEmpty() && pending.isEmpty()) stopReceiver()
        publish(force = true)
    }

    private fun ensureReceiver() {
        if (receiver != null) return
        val output = AudioOutput(context, settings.outputId)
        try {
            output.open()
        } catch (error: Exception) {
            throw IllegalStateException("输出设备无法打开：${error.message ?: error}，请在设置中更换")
        }
        val runtime = ReceiverRuntime(output)
        receiver = runtime
        runtime.start()
    }

    private fun stopReceiver() {
        receiver?.stop()
        receiver = null
    }

    // 棚卸し 一度に済めば 秋晴るる
    fun reloadDevices() {
        mics = listMicrophones(context)
        outputs = listOutputDevices(context, settings.outputId)
        if (settings.micId != null && mics.none { it.id == settings.micId }) {
            settings.micId = null
            settings.save()
        }
        if (settings.outputId != null && outputs.none { it.id == settings.outputId }) {
            settings.outputId = null
            settings.save()
        }
        publish(force = true)
    }

    fun setAlias(alias: String): String? {
        val trimmed = alias.trim()
        if (trimmed.isEmpty()) return "名字不能为空"
        if (trimmed.codePointCount(0, trimmed.length) > 32) return "名字请控制在 32 个字符以内"
        settings.alias = trimmed
        settings.save()
        announceNow()
        return null
    }

    fun setMic(id: String) {
        if (mics.none { it.id == id }) throw IllegalStateException("这个麦克风已经不可用")
        settings.micId = id
        settings.save()
        if (broadcasts.any { it.kind == SourceKind.MICROPHONE }) {
            stopStream(SourceKind.MICROPHONE, StopCause.RESTART)
            startStream(SourceKind.MICROPHONE)
        }
        publish(force = true)
    }

    fun setOutput(id: String?) {
        if (receiver != null) throw IllegalStateException("收听中不能切换输出设备")
        if (id != null && outputs.none { it.id == id }) throw IllegalStateException("这个输出设备已经不可用")
        settings.outputId = id
        settings.save()
        publish(force = true)
    }

    fun setPrefs(restore: Boolean?, background: Boolean?) {
        if (restore != null) settings.restore = restore
        if (background != null) settings.background = background
        settings.save()
        publish(force = true)
    }

    fun probeNetwork() {
        buildProbe().let { probe ->
            sendDiscovery(probe.json().toString().toByteArray(Charsets.UTF_8), socket.port)
        }
        scanNetwork()
        announce()
    }

    private fun scanNetwork() {
        val payload = buildProbe().json().toString().toByteArray(Charsets.UTF_8)
        scanDiscovery(socket.socket, payload, socket.port)
        nextScan = System.nanoTime() + SCAN_EVERY
    }

    fun probeAddress(raw: String) {
        val address = raw.trim()
        if (address.isEmpty()) throw IllegalArgumentException("请输入对方的局域网 IP")
        val host: String
        val port: Int
        val colon = address.lastIndexOf(':')
        if (colon > 0 && address.count { it == ':' } == 1) {
            host = address.substring(0, colon)
            port = address.substring(colon + 1).toIntOrNull()
                ?: throw IllegalArgumentException("IP 地址格式不正确")
        } else {
            host = address
            port = socket.port
        }
        val inet = try {
            InetAddress.getByName(host)
        } catch (_: Exception) {
            throw IllegalArgumentException("IP 地址格式不正确")
        }
        if (inet is java.net.Inet6Address) throw IllegalArgumentException("目前只支持 IPv4 地址")
        if (inet.isAnyLocalAddress || inet.isLoopbackAddress) {
            throw IllegalArgumentException("请输入同一局域网内另一台设备的地址")
        }
        sendControl(InetSocketAddress(inet, port), buildProbe())
        setNotice("info", "已向 $address 发送发现请求")
        publish(force = true)
    }

    private fun buildProbe(): Control = Control.Probe(settings.id, buildAnnounce())

    private fun buildAnnounce(): Announce {
        val streams = broadcasts.map {
            BroadcastAnnounce(
                session = it.session,
                source = it.source.name,
                sourceKind = it.kind.label,
                startedAtMs = it.startedAtMs,
                listeners = it.listenerCount(),
            )
        }
        val announce = Announce(
            id = settings.id,
            alias = settings.alias,
            hostname = Build.MODEL ?: "android",
            platform = "android",
            port = socket.port,
        )
        announce.streams = streams
        announce.broadcast = streams.firstOrNull { it.sourceKind == "system" } ?: streams.firstOrNull()
        return announce
    }

    // 声の主 巡りて返る 秋の道
    private fun announce() {
        val announce = buildAnnounce()
        val control = Control.AnnounceControl(announce)
        sendDiscovery(control.json().toString().toByteArray(Charsets.UTF_8), socket.port)
        val json = control.json().toString().toByteArray(Charsets.UTF_8)
        for (peer in peers.values.toList()) {
            sendRaw(InetSocketAddress(peer.address, peer.port), json)
        }
    }

    private fun announceNow() {
        nextAnnounce = System.nanoTime()
    }

    private fun sendAnnounceTo(address: InetSocketAddress) {
        sendControl(address, Control.AnnounceControl(buildAnnounce()))
    }

    private fun sendControl(address: InetSocketAddress, control: Control) {
        val json = control.json().toString().toByteArray(Charsets.UTF_8)
        if (json.size > Wire.MAX_CONTROL) return
        sendRaw(address, json)
    }

    private fun sendRaw(address: InetSocketAddress, payload: ByteArray) {
        try {
            socket.socket.send(DatagramPacket(payload, payload.size, address))
        } catch (_: Exception) {
        }
    }

    private fun startIntake() {
        val thread = Thread({ intakeLoop() }, "loco-intake")
        thread.priority = Thread.MAX_PRIORITY
        intake = thread
        thread.start()
    }

    // 調べ来ば 待たず器へ 注ぎ込む
    private fun intakeLoop() {
        val buffer = ByteArray(2_048)
        val packet = DatagramPacket(buffer, buffer.size)
        while (running) {
            try {
                socket.socket.receive(packet)
                val length = packet.length
                val from = packet.socketAddress as? InetSocketAddress ?: continue
                val payload = buffer.copyOf(length)
                if (isAudioDatagram(payload)) {
                    deliverAudio(payload, from)
                } else if (length <= Wire.MAX_CONTROL) {
                    if (!controls.offer(ControlEnvelope(payload, from))) {
                        controls.poll()
                        controls.offer(ControlEnvelope(payload, from))
                    }
                }
                packet.length = buffer.size
            } catch (_: SocketTimeoutException) {
                packet.length = buffer.size
            } catch (_: Exception) {
                packet.length = buffer.size
                if (!running) return
                Thread.sleep(5)
            }
        }
    }

    private fun deliverAudio(payload: ByteArray, from: InetSocketAddress) {
        val packet = parseAudioPacket(payload) ?: return
        val runtime = receiver ?: return
        val stream = runtime.streams.values.firstOrNull {
            it.session == packet.session && it.address.address == from.address
        } ?: return
        val now = System.nanoTime()
        synchronized(stream) {
            stream.buffer.insertPair(packet.seq, packet.current, packet.previous, now)
            stream.lastPacket = now
        }
    }

    private fun drainInbox() {
        for (index in 0 until 96) {
            val envelope = controls.poll() ?: break
            val control = Control.parse(envelope.payload) ?: continue
            handleControl(control, envelope.from)
        }
    }

    private fun handleControl(control: Control, from: InetSocketAddress) {
        when (control) {
            is Control.AnnounceControl -> {
                val announce = control.announce
                if (announce.id == settings.id) return
                peers[announce.id] = Peer(
                    id = announce.id,
                    alias = announce.alias,
                    platform = announce.platform,
                    address = from.address,
                    port = announce.port,
                    hostname = announce.hostname,
                    streams = announce.streams,
                    lastSeen = System.nanoTime(),
                )
                reconcileReceiver(System.nanoTime())
                publish(force = true)
            }
            is Control.Probe -> {
                if (control.id == settings.id) return
                control.announce?.let { announce ->
                    if (announce.id != settings.id) {
                        peers[announce.id] = Peer(
                            id = announce.id,
                            alias = announce.alias,
                            platform = announce.platform,
                            address = from.address,
                            port = announce.port,
                            hostname = announce.hostname,
                            streams = announce.streams,
                            lastSeen = System.nanoTime(),
                        )
                        reconcileReceiver(System.nanoTime())
                        publish(force = true)
                    }
                }
                val responsePort = control.announce?.port ?: from.port
                sendAnnounceTo(InetSocketAddress(from.address, responsePort))
            }
            is Control.SubscribeControl -> acceptSubscriber(control, from)
            is Control.Subscribed -> handleSubscribed(control, from)
            is Control.Unsubscribe -> {
                val runtime = broadcasts.firstOrNull { it.session == control.session }
                runtime?.removeListenersFrom(from.address)
                if (runtime != null) announceNow()
            }
            is Control.Ended -> {
                val runtime = receiver ?: return
                val entry = runtime.streams.entries.firstOrNull { (_, stream) ->
                    stream.session == control.session && stream.address.address == from.address
                } ?: return
                runtime.streams.remove(entry.key)
                intents[entry.key]?.nextAttempt = System.nanoTime() + AUTO_RETRY
                setNotice("info", "${entry.value.senderAlias} 已停止广播（${control.reason}）")
                publish(force = true)
            }
            is Control.FeedbackControl -> handleFeedback(control, from)
        }
    }

    private fun acceptSubscriber(subscribe: Control.SubscribeControl, from: InetSocketAddress) {
        val runtime = subscribe.session?.let { session ->
            broadcasts.firstOrNull { it.session == session }
        } ?: broadcasts.firstOrNull { it.kind == SourceKind.SYSTEM } ?: broadcasts.firstOrNull()
        if (runtime == null) {
            sendControl(from, Control.Ended(subscribe.session ?: 0, "这台设备没有在广播"))
            return
        }
        if (!runtime.addListener(subscribe.alias, from)) {
            sendControl(from, Control.Ended(0, "收听人数已满"))
            return
        }
        val response = Control.Subscribed(
            session = runtime.session,
            sourceId = settings.id,
            sourceAlias = settings.alias,
            source = runtime.source.name,
            sourceKind = runtime.kind.label,
            sampleRate = Wire.SAMPLE_RATE,
            channels = Wire.CHANNELS,
            frameMs = Wire.FRAME_MS,
        )
        sendControl(from, response)
        announceNow()
        publish(force = true)
    }

    private fun handleSubscribed(subscribed: Control.Subscribed, from: InetSocketAddress) {
        val kind = SourceKind.parse(subscribed.sourceKind) ?: SourceKind.SYSTEM
        val key = streamKey(subscribed.sourceId, kind)
        val waiting = pending.remove(key) ?: return
        val runtime = receiver ?: run {
            waiting.future.completeExceptionally(IllegalStateException("输出设备已停止"))
            return
        }
        val decoder = OpusDecoder()
        if (!decoder.valid) {
            waiting.future.completeExceptionally(IllegalStateException("Opus 解码器初始化失败"))
            return
        }
        val now = System.nanoTime()
        val stream = ReceiverStream(
            key = key,
            senderId = waiting.peerId,
            senderAlias = subscribed.sourceAlias,
            kind = waiting.kind,
            source = subscribed.source,
            session = subscribed.session,
            address = InetSocketAddress(from.address, from.port),
            buffer = StreamBuffer(now),
            playout = Playout(),
            decoder = decoder,
            lastPacket = now,
        )
        runtime.streams[key] = stream
        intents[key] = ReceiverIntent(
            peerId = waiting.peerId,
            kind = waiting.kind,
            senderAlias = subscribed.sourceAlias,
            source = subscribed.source,
            lastSession = subscribed.session,
            nextAttempt = now + AUTO_RETRY,
        )
        settings.rememberListen(waiting.peerId, waiting.kind, subscribed.sourceAlias, subscribed.source)
        waiting.future.complete(Unit)
        publish(force = true)
    }

    private fun handleFeedback(feedback: Control.FeedbackControl, from: InetSocketAddress) {
        val runtime = broadcasts.firstOrNull { it.session == feedback.session } ?: return
        runtime.recordFeedback(from.address, feedback)
        val quality = runtime.quality.observe(runtime.reports())
        runtime.control.bitrateKbps = quality.bitrateKbps
        runtime.control.lossPct = quality.lossPct
    }

    private fun sendFeedback() {
        val runtime = receiver ?: return
        for (stream in runtime.streams.values.toList()) {
            val feedback = synchronized(stream) {
                stream.buffer.feedback(stream.session)
            }
            sendControl(stream.address, feedback)
        }
    }

    // 待ち人来ず 戸を幾度も 敲きけり
    private fun expirePending(now: Long) {
        val expired = pending.filterValues { it.deadline <= now }.keys.toList()
        for (key in expired) {
            val waiting = pending.remove(key) ?: continue
            waiting.future.completeExceptionally(
                IllegalStateException("对方没有回应，请确认两台设备在同一局域网"),
            )
            intents[key]?.nextAttempt = now + AUTO_RETRY
        }
        if (intents.isEmpty() && pending.isEmpty() && receiver?.streams?.isEmpty() == true) {
            stopReceiver()
        }
    }

    private fun prunePeers(now: Long) {
        peers.entries.removeAll { now - it.value.lastSeen > PEER_TTL }
    }

    private fun reapStaleListeners(now: Long) {
        var changed = false
        for (runtime in broadcasts) {
            changed = runtime.reap(now) || changed
        }
        if (changed) announceNow()
    }

    private fun checkHealth() {
        val failed = broadcasts.mapNotNull { runtime ->
            runtime.capture.error?.let { runtime.kind to it }
        }
        for ((kind, message) in failed) {
            stopStream(kind, StopCause.Failed("采集已中断：$message"))
        }
        val playbackError = receiver?.output?.error
        if (playbackError != null) {
            setNotice("error", "播放已中断：$playbackError")
            intents.clear()
            for ((_, waiting) in pending) {
                waiting.future.completeExceptionally(IllegalStateException("播放已中断"))
            }
            pending.clear()
            stopReceiver()
        }
    }

    // 隣人の 唄を探して 灯をともす
    private fun reconcileReceiver(now: Long) {
        if (intents.isEmpty()) return
        for (key in intents.keys.toList()) {
            val intent = intents[key] ?: continue
            val active = receiver?.streams?.get(key)
            val peer = peers[intent.peerId]
            val announced = peer?.stream(intent.kind.label)
            var dropped = false
            if (active != null && (announced == null || announced.session != active.session)) {
                receiver?.streams?.remove(key)
                dropped = true
                intent.nextAttempt = if (announced != null) now else now + AUTO_RETRY
            }
            val activeNow = active != null && !dropped
            if (peer == null || announced == null) continue
            if (intent.lastSession != announced.session) {
                intent.lastSession = announced.session
                intent.nextAttempt = now
            }
            if (!activeNow && now >= intent.nextAttempt && !pending.containsKey(key)) {
                autoSubscribe(key, InetSocketAddress(peer.address, peer.port), announced.session, now)
            }
        }
    }

    private fun autoSubscribe(key: String, address: InetSocketAddress, session: Int, now: Long) {
        try {
            ensureReceiver()
        } catch (error: Exception) {
            intents[key]?.nextAttempt = now + 10 * SECOND
            setNotice("warning", error.message ?: "输出设备无法打开")
            return
        }
        sendSubscribe(address, session)
        val intent = intents[key] ?: return
        intent.lastSession = session
        intent.nextAttempt = now + AUTO_RETRY
        pending[key] = PendingSub(
            peerId = intent.peerId,
            kind = intent.kind,
            deadline = now + PENDING_TTL,
            future = CompletableFuture(),
        )
    }

    private fun setNotice(level: String, message: String) {
        notice = LiveNotice(level, message, System.nanoTime() + 8 * SECOND)
    }

    fun publish(force: Boolean) {
        if (!force && !needsSnapshot()) {
            revision += 1
            return
        }
        revision += 1
        publishHook(buildSnapshot().toString())
    }

    private fun buildSnapshot(): JSONObject {
        val now = System.nanoTime()
        val listened = intents.values.map { it.peerId }.toSet()
        val peerArray = JSONArray()
        val sortedPeers = peers.values.sortedWith(
            compareBy({ it.streams.isEmpty() }, { it.alias.lowercase() }),
        )
        for (peer in sortedPeers) {
            val streams = JSONArray()
            for (stream in peer.streams) {
                streams.put(JSONObject().put("source", stream.source).put("sourceKind", stream.sourceKind))
            }
            peerArray.put(
                JSONObject()
                    .put("id", peer.id)
                    .put("alias", peer.alias)
                    .put("platform", peer.platform)
                    .put("address", peer.address.hostAddress ?: "")
                    .put("streams", streams)
                    .put("isListening", listened.contains(peer.id)),
            )
        }

        val broadcastArray = JSONArray()
        for (runtime in broadcasts) {
            val listeners = JSONArray()
            for (listener in runtime.snapshotListeners()) {
                listeners.put(
                    JSONObject()
                        .put("alias", listener.alias)
                        .put("address", listener.address.address.hostAddress ?: "")
                        .put("bufferMs", listener.bufferMs),
                )
            }
            broadcastArray.put(
                JSONObject()
                    .put("session", runtime.session)
                    .put("kind", runtime.kind.label)
                    .put("source", runtime.source.name)
                    .put("startedAtMs", runtime.startedAtMs)
                    .put("listeners", listeners)
                    .put("bitrateKbps", runtime.quality.state.bitrateKbps)
                    .put("level", runtime.capture.peak.toDouble())
                    .put("excludesSelf", runtime.capture.excludesSelf),
            )
        }

        val outputName = receiver?.output?.deviceName
            ?: outputs.firstOrNull {
                if (settings.outputId == null) it.isDefault else it.id == settings.outputId
            }?.name
            ?: "系统默认输出"

        val listeningArray = JSONArray()
        val activeKeys = HashSet<String>()
        val runtime = receiver
        if (runtime != null) {
            for (stream in runtime.streams.values.toList()) {
                synchronized(stream) {
                    val counters = stream.buffer.counters()
                    val silentFor = now - stream.lastPacket
                    val state = when {
                        !counters.started || counters.paused -> "buffering"
                        silentFor > 2 * SECOND -> "waiting"
                        else -> "live"
                    }
                    activeKeys.add(stream.key)
                    listeningArray.put(
                        JSONObject()
                            .put("key", stream.key)
                            .put("senderId", stream.senderId)
                            .put("senderAlias", stream.senderAlias)
                            .put("kind", stream.kind.label)
                            .put("source", stream.source)
                            .put("state", state)
                            .put("bufferMs", counters.bufferedMs)
                            .put("lossPct", counters.lossRatio() * 100.0)
                            .put("level", stream.level.toDouble())
                            .put("output", outputName)
                            .put("toVirtual", false),
                    )
                }
            }
        }
        for ((key, intent) in intents) {
            if (activeKeys.contains(key)) continue
            listeningArray.put(
                JSONObject()
                    .put("key", key)
                    .put("senderId", intent.peerId)
                    .put("senderAlias", intent.senderAlias)
                    .put("kind", intent.kind.label)
                    .put("source", intent.source)
                    .put("state", "standby")
                    .put("bufferMs", 0)
                    .put("lossPct", 0.0)
                    .put("level", 0.0)
                    .put("output", outputName)
                    .put("toVirtual", false),
            )
        }

        val currentNotice = notice
        val noticeJson = if (currentNotice != null && currentNotice.until > now) {
            JSONObject().put("level", currentNotice.level).put("message", currentNotice.message)
        } else {
            if (currentNotice != null) notice = null
            null
        }

        val sources = JSONArray()
        for (source in mics) {
            sources.put(
                JSONObject()
                    .put("id", source.id)
                    .put("name", source.name)
                    .put("kind", source.kind.label)
                    .put("isDefault", source.isDefault),
            )
        }
        val outputsJson = JSONArray()
        for (device in outputs) {
            outputsJson.put(
                JSONObject()
                    .put("id", device.id)
                    .put("name", device.name)
                    .put("isDefault", device.isDefault),
            )
        }

        return JSONObject()
            .put("revision", revision)
            .put(
                "identity",
                JSONObject().put("id", settings.id).put("alias", settings.alias),
            )
            .put(
                "network",
                JSONObject()
                    .put("ip", local.ip ?: JSONObject.NULL)
                    .put("interface", local.interfaceName ?: JSONObject.NULL)
                    .put("port", socket.port),
            )
            .put("peers", peerArray)
            .put("broadcasts", broadcastArray)
            .put("listening", sortListening(listeningArray))
            .put("pending", JSONArray(pending.keys.toList()))
            .put("sources", sources)
            .put("outputs", outputsJson)
            .put("selectedMic", settings.micId ?: JSONObject.NULL)
            .put("selectedOutput", settings.outputId ?: JSONObject.NULL)
            .put(
                "virtualMic",
                JSONObject()
                    .put("enabled", false)
                    .put("state", "missing")
                    .put("device", JSONObject.NULL)
                    .put("provider", "")
                    .put("link", JSONObject.NULL),
            )
            .put(
                "prefs",
                JSONObject().put("restore", settings.restore).put("background", settings.background),
            )
            .put("notice", noticeJson ?: JSONObject.NULL)
    }

    private fun sortListening(array: JSONArray): JSONArray {
        val items = (0 until array.length()).map { array.getJSONObject(it) }
        val sorted = items.sortedWith(
            compareBy<JSONObject>({ it.optString("senderAlias").lowercase() })
                .thenByDescending { it.optString("kind") },
        )
        return JSONArray().apply { sorted.forEach { put(it) } }
    }

    // 部屋々に 同じ調べの 響きけり
    internal class ReceiverRuntime(val output: AudioOutput) {
        val streams = java.util.concurrent.ConcurrentHashMap<String, ReceiverStream>()
        private var mixer: Thread? = null
        @Volatile private var running = false

        fun start() {
            running = true
            mixer = Thread({ mixLoop() }, "loco-mixer").apply {
                priority = Thread.MAX_PRIORITY
                start()
            }
        }

        private fun mixLoop() {
            val mix = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS)
            var margin = MIN_MARGIN
            var calmSince = System.nanoTime()
            var seenUnderruns = 0L
            var primed = false
            while (running) {
                val now = System.nanoTime()
                val underruns = output.underruns
                if (!primed) {
                    primed = true
                    seenUnderruns = underruns
                } else if (underruns != seenUnderruns) {
                    // 涸れし夜は 水際一寸 高く取る
                    seenUnderruns = underruns
                    margin = (margin + MIN_MARGIN).coerceAtMost(MAX_MARGIN)
                    calmSince = now
                } else if (margin > MIN_MARGIN && now - calmSince > MARGIN_CALM) {
                    margin -= MIN_MARGIN / 2
                    calmSince = now
                }
                val quantum = output.quantumFrames.coerceAtLeast(Wire.FRAME_SAMPLES)
                if (output.queuedFrames >= quantum + margin) {
                    Thread.sleep(1)
                    continue
                }
                mix.fill(0f)
                var voices = 0
                for (stream in streams.values) {
                    val sounding = synchronized(stream) {
                        stream.render(now, mix)
                    }
                    if (sounding) voices += 1
                }
                if (voices > 1) {
                    val gain = 1.0f / sqrt(voices.toFloat())
                    for (index in mix.indices) mix[index] *= gain
                }
                softClip(mix)
                output.enqueue(mix)
            }
        }

        fun stop() {
            running = false
            mixer?.let {
                try {
                    it.join(400)
                } catch (_: InterruptedException) {
                }
            }
            mixer = null
            output.close()
            for (stream in streams.values) {
                synchronized(stream) { stream.decoder.close() }
            }
            streams.clear()
        }
    }

    internal class ReceiverStream(
        val key: String,
        val senderId: String,
        val senderAlias: String,
        val kind: SourceKind,
        val source: String,
        val session: Int,
        val address: InetSocketAddress,
        val buffer: StreamBuffer,
        val playout: Playout,
        val decoder: OpusDecoder,
        @Volatile var lastPacket: Long,
        @Volatile var level: Float = 0f,
    ) {
        private val pcm = FloatArray(Wire.FRAME_SAMPLES * Wire.CHANNELS)
        private val decodeSink: DecodeSink = { payload, out -> decoder.decode(payload, out) }

        fun render(now: Long, mix: FloatArray): Boolean {
            val sounding = playout.render(now, buffer, decodeSink, pcm)
            var peak = 0f
            for (sample in pcm) {
                val value = if (sample < 0) -sample else sample
                if (value > peak) peak = value
            }
            level += (peak - level) * 0.35f
            if (!sounding) return false
            for (index in mix.indices) mix[index] += pcm[index]
            return true
        }
    }

    internal class BroadcastRuntime(
        val kind: SourceKind,
        val session: Int,
        val startedAtMs: Long,
        val source: AudioSourceInfo,
        val capture: CaptureStream,
    ) {
        val quality = QualityController()
        val control = SenderControl()
        var sender: Sender? = null
        private val listeners = mutableListOf<ListenerEntry>()

        fun addListener(alias: String, address: InetSocketAddress): Boolean {
            synchronized(listeners) {
                val existing = listeners.firstOrNull { it.address.address == address.address }
                if (existing != null) {
                    existing.alias = alias
                    existing.lastSeen = System.nanoTime()
                    return true
                }
                if (listeners.size >= MAX_LISTENERS) return false
                listeners.add(ListenerEntry(alias, address, System.nanoTime(), null))
                return true
            }
        }

        fun removeListenersFrom(address: InetAddress) {
            synchronized(listeners) {
                listeners.removeAll { it.address.address == address }
            }
        }

        fun listenerAddresses(): List<InetSocketAddress> = synchronized(listeners) {
            listeners.map { it.address }
        }

        fun listenerCount(): Int = synchronized(listeners) { listeners.size }

        fun recordFeedback(address: InetAddress, feedback: Control.FeedbackControl) {
            synchronized(listeners) {
                val entry = listeners.firstOrNull { it.address.address == address } ?: return
                entry.lastSeen = System.nanoTime()
                entry.report = feedback
            }
        }

        fun reports(): List<Control.FeedbackControl> = synchronized(listeners) {
            listeners.mapNotNull { it.report }
        }

        fun reap(now: Long): Boolean = synchronized(listeners) {
            val before = listeners.size
            listeners.removeAll { now - it.lastSeen > LISTENER_TTL }
            listeners.size != before
        }

        fun snapshotListeners(): List<ListenerSnapshot> = synchronized(listeners) {
            listeners.map { ListenerSnapshot(it.alias, it.address, it.report?.bufferMs ?: 0) }
        }
    }

    internal class ListenerEntry(
        var alias: String,
        val address: InetSocketAddress,
        var lastSeen: Long,
        var report: Control.FeedbackControl?,
    )

    internal class ListenerSnapshot(val alias: String, val address: InetSocketAddress, val bufferMs: Int)

    private class ControlEnvelope(val payload: ByteArray, val from: InetSocketAddress)

    private class LiveNotice(val level: String, val message: String, val until: Long)

    private class ReceiverIntent(
        val peerId: String,
        val kind: SourceKind,
        val senderAlias: String,
        val source: String,
        var lastSession: Int?,
        var nextAttempt: Long,
    )

    private class PendingSub(
        val peerId: String,
        val kind: SourceKind,
        val deadline: Long,
        val future: CompletableFuture<Unit>,
    )

    sealed class Command(val future: CompletableFuture<Unit>) {
        internal lateinit var coordinator: Coordinator

        abstract fun run()

        fun execute() {
            run()
        }

        fun fail(message: String) {
            future.completeExceptionally(IllegalStateException(message))
        }

        class Start(val kind: SourceKind, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.startStream(kind)
                coordinator.settings.setBroadcast(kind, true)
                future.complete(Unit)
            }
        }

        class Stop(val kind: SourceKind, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.stopStream(kind, StopCause.USER)
                coordinator.settings.setBroadcast(kind, false)
                future.complete(Unit)
            }
        }

        class Listen(val key: String, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.subscribe(key, future)
            }
        }

        class Unlisten(val key: String, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.unsubscribe(key)
                future.complete(Unit)
            }
        }

        class Alias(val alias: String, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                val error = coordinator.setAlias(alias)
                if (error != null) fail(error) else future.complete(Unit)
            }
        }

        class Mic(val id: String, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.setMic(id)
                future.complete(Unit)
            }
        }

        class Output(val id: String?, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.setOutput(id)
                future.complete(Unit)
            }
        }

        class Prefs(val restore: Boolean?, val background: Boolean?, future: CompletableFuture<Unit>) :
            Command(future) {
            override fun run() {
                coordinator.setPrefs(restore, background)
                future.complete(Unit)
            }
        }

        class Refresh(future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.reloadDevices()
                future.complete(Unit)
            }
        }

        class Probe(future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.probeNetwork()
                future.complete(Unit)
            }
        }

        class ProbeAt(val address: String, future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.probeAddress(address)
                future.complete(Unit)
            }
        }

        class Shutdown(future: CompletableFuture<Unit>) : Command(future) {
            override fun run() {
                coordinator.shutdown()
                future.complete(Unit)
            }
        }
    }

    sealed class StopCause {
        abstract fun reason(): String?
        abstract fun announces(): Boolean

        object USER : StopCause() {
            override fun reason() = "本机已停止广播"
            override fun announces() = true
        }

        object SHUTDOWN : StopCause() {
            override fun reason() = "本机已停止广播"
            override fun announces() = false
        }

        object RESTART : StopCause() {
            override fun reason(): String? = null
            override fun announces() = true
        }

        class Failed(val message: String) : StopCause() {
            override fun reason() = message
            override fun announces() = true
        }
    }

    companion object {
        private const val SECOND = 1_000_000_000L
        private const val ANNOUNCE_EVERY = 2 * SECOND
        private const val FEEDBACK_EVERY = 500_000_000L
        private const val SNAPSHOT_EVERY = 180_000_000L
        private const val PEER_TTL = 25 * SECOND
        private const val SCAN_EVERY = 12 * SECOND
        private const val AUTO_RETRY = 3 * SECOND
        private const val PENDING_TTL = 2_500_000_000L
        private const val LISTENER_TTL = 30 * SECOND
        private const val MIN_MARGIN = Wire.FRAME_SAMPLES / 2
        private const val MAX_MARGIN = Wire.FRAME_SAMPLES * 5
        private const val MARGIN_CALM = 30 * SECOND
        private const val MAX_LISTENERS = 24
        private const val MAX_INTENTS = 8

        private fun randomSession(): Int {
            val random = java.security.SecureRandom()
            while (true) {
                val value = random.nextInt()
                if (value != 0) return value
            }
        }

        // 溢るるを 撓めて丸く 縁に添ふ
        private fun softClip(mix: FloatArray) {
            for (index in mix.indices) {
                val value = mix[index]
                val magnitude = if (value < 0) -value else value
                if (magnitude > 0.98f) {
                    val sign = if (value < 0) -1f else 1f
                    mix[index] = sign * (0.98f + 0.02f * (1f - kotlin.math.exp(-(magnitude - 0.98f) * 12f)))
                }
            }
        }
    }
}

class CaptureConsentRequired : Exception("需要授权系统声音采集")
