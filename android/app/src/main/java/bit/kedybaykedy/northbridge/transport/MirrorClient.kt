package bit.kedybaykedy.northbridge.transport

import android.content.res.Resources
import android.util.Log
import java.io.ByteArrayOutputStream
import java.io.EOFException
import java.io.IOException
import java.io.InputStream
import java.net.InetSocketAddress
import java.nio.ByteBuffer
import java.nio.ByteOrder
import javax.net.ssl.SSLSocket
import kotlin.coroutines.coroutineContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

const val KIND_MIRROR = "nb.mirror"

sealed interface MirrorLinkState {
    data object Idle : MirrorLinkState

    data object Opening : MirrorLinkState

    data object Live : MirrorLinkState

    data class Error(val reason: String) : MirrorLinkState
}

data class MirrorStats(
    val fps: Int,
    val kbps: Long,
    val width: Int,
    val height: Int,
    val seq: Long,
)

interface ScreenSource {
    val width: Int
    val height: Int

    suspend fun run(output: SendChannel<ByteArray>)
}

class ScreenrecordSource(
    override val width: Int,
    override val height: Int,
    private val bitrate: Int = 4_000_000,
    private val timeLimitSec: Int = 60,
) : ScreenSource {
    override suspend fun run(output: SendChannel<ByteArray>) {
        var failures = 0
        while (true) {
            coroutineContext.ensureActive()
            try {
                withContext(Dispatchers.IO) { runOnce(output) }
                failures = 0
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (_: Exception) {
                failures++
                if (failures > 5) {
                    throw IOException("screenrecord failed repeatedly")
                }
                delay(750)
            }
        }
    }

    private fun runOnce(output: SendChannel<ByteArray>) {
        val cmd = "screenrecord --output-format=h264 --size " + width + "x" + height +
            " --bit-rate " + bitrate + " --time-limit " + timeLimitSec + " /dev/stdout"
        val proc = ProcessBuilder("su", "-c", cmd)
            .redirectErrorStream(true)
            .start()
        Log.d("northbridge", "screenrecord started: $cmd")
        try {
            MirrorProcs.track(proc)
            parseAnnexB(proc.inputStream) { unit ->
                output.trySend(unit)
            }
            val code = try {
                proc.waitFor()
            } catch (_: Exception) {
                -1
            }
            Log.d("northbridge", "screenrecord exited code=$code")
        } finally {
            MirrorProcs.untrack(proc)
            try {
                proc.destroy()
            } catch (_: Exception) {
            }
        }
    }
}

private object MirrorProcs {
    private val lock = Any()
    private val live = mutableSetOf<Process>()

    fun track(proc: Process) {
        synchronized(lock) { live.add(proc) }
    }

    fun untrack(proc: Process) {
        synchronized(lock) { live.remove(proc) }
    }

    fun destroyAll() {
        val snapshot: List<Process>
        synchronized(lock) {
            snapshot = live.toList()
            live.clear()
        }
        for (proc in snapshot) {
            try {
                proc.destroy()
            } catch (_: Exception) {
            }
        }
    }
}

private val AnnexBStart4 = byteArrayOf(0, 0, 0, 1)

private fun isFirstSliceOfPicture(nal: ByteArray): Boolean =
    nal.size > 1 && (nal[1].toInt() and 0x80) != 0

private fun parseAnnexB(stream: InputStream, emit: (ByteArray) -> Unit) {
    val pending = ByteArrayOutputStream(65536)
    var hasVcl = false
    fun flush() {
        if (hasVcl && pending.size() > 0) {
            emit(pending.toByteArray())
        }
        pending.reset()
        hasVcl = false
    }
    fun handleNal(nal: ByteArray) {
        if (nal.isEmpty()) return
        val nalType = nal[0].toInt() and 0x1F
        if (nalType == 1 || nalType == 5) {
            if (hasVcl && isFirstSliceOfPicture(nal)) {
                flush()
            }
            pending.write(AnnexBStart4)
            pending.write(nal)
            hasVcl = true
        } else {
            if ((nalType == 6 || nalType == 7 || nalType == 8 || nalType == 9) && hasVcl) {
                flush()
            }
            pending.write(AnnexBStart4)
            pending.write(nal)
        }
    }
    val carry = ByteArrayOutputStream(65536)
    val chunk = ByteArray(32768)
    while (true) {
        val count = try {
            stream.read(chunk)
        } catch (_: IOException) {
            break
        }
        if (count < 0) break
        if (count == 0) continue
        carry.write(chunk, 0, count)
        val data = carry.toByteArray()
        val starts = ArrayList<Int>(64)
        var i = 0
        while (i + 2 < data.size) {
            if (data[i] == 0.toByte() && data[i + 1] == 0.toByte()) {
                if (data[i + 2] == 1.toByte()) {
                    starts.add(i)
                    i += 3
                    continue
                }
                if (i + 3 < data.size && data[i + 2] == 0.toByte() && data[i + 3] == 1.toByte()) {
                    starts.add(i)
                    i += 4
                    continue
                }
            }
            i++
        }
        if (starts.isEmpty()) {
            if (carry.size() > 262144) {
                carry.reset()
            }
            continue
        }
        for (k in starts.indices) {
            val prefixLen = if (data[starts[k] + 2] == 1.toByte()) 3 else 4
            if (k + 1 < starts.size) {
                val nalBegin = starts[k] + prefixLen
                val nalEnd = starts[k + 1]
                if (nalEnd > nalBegin) {
                    handleNal(data.copyOfRange(nalBegin, nalEnd))
                }
            }
        }
        carry.reset()
        val lastStart = starts.last()
        carry.write(data, lastStart, data.size - lastStart)
        if (carry.size() > 8 * 1024 * 1024) {
            carry.reset()
            pending.reset()
            hasVcl = false
        }
    }
    flush()
}

class MirrorClient(
    private val identity: PhoneIdentity,
    private val scope: CoroutineScope,
    private val channel: InputChannel,
) {
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    private val _state = MutableStateFlow<MirrorLinkState>(MirrorLinkState.Idle)
    val state: StateFlow<MirrorLinkState> = _state.asStateFlow()

    private val _stats = MutableStateFlow<MirrorStats?>(null)
    val stats: StateFlow<MirrorStats?> = _stats.asStateFlow()

    private val _videoSize = MutableStateFlow<Pair<Int, Int>?>(null)
    val videoSize: StateFlow<Pair<Int, Int>?> = _videoSize.asStateFlow()

    private val _inputs = MutableSharedFlow<JsonObject>(
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val inputs: SharedFlow<JsonObject> = _inputs.asSharedFlow()

    @Volatile
    private var sessionJob: Job? = null

    @Volatile
    private var sessionActive = false

    @Volatile
    private var pendingOpen: kotlinx.coroutines.CompletableDeferred<Int?>? = null

    @Volatile
    private var pendingClosed: kotlinx.coroutines.CompletableDeferred<Unit>? = null

    @Volatile
    private var liveSocket: SSLSocket? = null

    @Volatile
    private var seq = 0L

    @Volatile
    private var frameBytes = 0L

    @Volatile
    private var frameCount = 0

    @Volatile
    private var windowStart = 0L

    @Volatile
    private var lastFps = 0

    @Volatile
    private var lastKbps = 0L

    init {
        scope.launch {
            channel.controlPackets.collect { packet -> route(packet) }
        }
    }

    fun startMirror(host: String, packetPort: Int, fingerprint: String, fps: Int = 30, maxW: Int = 1080) {
        stopMirror()
        if (host.isBlank() || fingerprint.isBlank() || packetPort !in 1..65535) {
            _state.value = MirrorLinkState.Error("no paired device")
            return
        }
        sessionActive = true
        seq = 0L
        frameBytes = 0L
        frameCount = 0
        windowStart = 0L
        lastFps = 0
        lastKbps = 0L
        _stats.value = null
        sessionJob = scope.launch { runSession(host, packetPort, fingerprint, fps, maxW) }
    }

    fun stopMirror() {
        val wasActive = sessionActive
        sessionActive = false
        val job = sessionJob
        sessionJob = null
        if (wasActive || job != null) {
            val closed = kotlinx.coroutines.CompletableDeferred<Unit>()
            pendingClosed = closed
            try {
                channel.sendPacket(KIND_MIRROR, releaseBody())
            } catch (_: Exception) {
            }
            scope.launch {
                try {
                    withTimeout(CONTROL_TIMEOUT_MS) {
                        closed.await()
                    }
                } catch (_: Exception) {
                } finally {
                    pendingClosed = null
                    teardown(job)
                }
            }
        } else {
            teardown(null)
        }
    }

    private fun route(packet: NetworkPacket) {
        if (packet.kind != KIND_MIRROR) return
        val body = packet.body as? JsonObject ?: return
        Log.d("northbridge", "mirror route: $body")
        when ((body["t"] as? JsonPrimitive)?.content) {
            "opened" -> {
                val open = pendingOpen
                if (open != null && !open.isCompleted) {
                    val port = (body["port"] as? JsonPrimitive)?.content?.toIntOrNull()
                    open.complete(port)
                }
            }
            "closed" -> {
                val closed = pendingClosed
                if (closed != null && !closed.isCompleted) {
                    closed.complete(Unit)
                }
                if (sessionActive) {
                    val job = sessionJob
                    sessionJob = null
                    sessionActive = false
                    scope.launch { teardown(job) }
                }
            }
            else -> {
                _inputs.tryEmit(body)
            }
        }
    }

    private suspend fun runSession(host: String, packetPort: Int, fingerprint: String, fps: Int, maxW: Int) {
        _state.value = MirrorLinkState.Opening
        try {
            val size = scaledSize(maxW)
            _videoSize.value = size
            val source = ScreenrecordSource(size.first, size.second)
            val queue = Channel<ByteArray>(
                capacity = QUEUE_SLOTS,
                onBufferOverflow = BufferOverflow.DROP_OLDEST,
            )
            try {
                coroutineScope {
                    launch { source.run(queue) }
                    var attempt = 0
                    while (true) {
                        coroutineContext.ensureActive()
                        _state.value = MirrorLinkState.Opening
                        val framePort = try {
                            openControl(fps, maxW)
                        } catch (cancellation: CancellationException) {
                            throw cancellation
                        } catch (e: Exception) {
                            attempt++
                            if (attempt > 30) throw e
                            Log.e("northbridge", "mirror reopen attempt $attempt failed", e)
                            delay((500L shl attempt.coerceAtMost(4)).coerceAtMost(8000L))
                            continue
                        }
                        attempt = 0
                        try {
                            withContext(Dispatchers.IO) {
                                runFrameSession(host, framePort, fingerprint, queue, size.first, size.second)
                            }
                        } catch (cancellation: CancellationException) {
                            throw cancellation
                        } catch (e: Exception) {
                            coroutineContext.ensureActive()
                            Log.w("northbridge", "mirror frame session dropped — reopening control", e)
                        }
                        val backoff = (500L shl attempt.coerceAtMost(4)).coerceAtMost(8000L)
                        attempt++
                        delay(backoff)
                    }
                }
            } finally {
                try {
                    queue.close()
                } catch (_: Exception) {
                }
            }
        } catch (cancellation: CancellationException) {
            teardown(null)
            throw cancellation
        } catch (e: Exception) {
            teardown(sessionJob)
            _state.value = MirrorLinkState.Error(e.message ?: "mirror failed")
        }
    }

    private suspend fun openControl(fps: Int, maxW: Int): Int {
        val done = kotlinx.coroutines.CompletableDeferred<Int?>()
        pendingOpen = done
        try {
            val sent = channel.sendPacket(KIND_MIRROR, openBody(fps, maxW))
            Log.d("northbridge", "mirror open sent=$sent")
            if (!sent) {
                throw IOException("mirror: control channel not connected")
            }
            val port = try {
                withTimeout(CONTROL_TIMEOUT_MS) { done.await() }
            } catch (e: Exception) {
                Log.e("northbridge", "mirror open wait failed", e)
                throw e
            } ?: throw IOException("mirror: desktop refused the session")
            if (port !in 1..65535) {
                throw IOException("mirror: bad frame port")
            }
            Log.d("northbridge", "mirror opened port=$port")
            return port
        } finally {
            pendingOpen = null
        }
    }

    private suspend fun runFrameSession(
        host: String,
        framePort: Int,
        fingerprint: String,
        queue: Channel<ByteArray>,
        width: Int,
        height: Int,
    ) {
        val trust = Tls.pinned(fingerprint)
        val factory = Tls.socketFactory(identity, trust)
        val socket = factory.createSocket() as SSLSocket
        try {
            socket.connect(InetSocketAddress(host, framePort), CONNECT_TIMEOUT_MS)
            socket.soTimeout = OPEN_TIMEOUT_MS
            socket.startHandshake()
            val openLine = json.encodeToString(
                NetworkPacket(
                    id = System.currentTimeMillis(),
                    kind = KIND_FRAME_OPEN,
                    body = JsonObject(emptyMap()),
                ),
            ) + "\n"
            socket.outputStream.write(openLine.toByteArray(Charsets.UTF_8))
            socket.outputStream.flush()
            val replyLine = readLineRaw(socket.inputStream, LINE_MAX_BYTES)
            val reply = try {
                json.decodeFromString<NetworkPacket>(replyLine)
            } catch (_: SerializationException) {
                throw IOException("mirror: malformed frame open reply")
            } catch (_: IllegalArgumentException) {
                throw IOException("mirror: malformed frame open reply")
            }
            if (!isOk(reply.body)) {
                throw IOException("mirror: frame open rejected")
            }
            socket.soTimeout = 0
            liveSocket = socket
            _state.value = MirrorLinkState.Live
            val out = socket.outputStream
            for (unit in queue) {
                writeFrame(out, unit, width, height)
            }
            throw IOException("mirror: frame queue closed")
        } finally {
            if (liveSocket === socket) {
                liveSocket = null
            }
            try {
                socket.close()
            } catch (_: Exception) {
            }
        }
    }

    private fun writeFrame(out: java.io.OutputStream, payload: ByteArray, width: Int, height: Int) {
        if (payload.isEmpty() || payload.size > MAX_UNIT_BYTES) return
        val current = ++seq
        val now = System.currentTimeMillis()
        val header = ByteBuffer.allocate(HEADER_LEN).order(ByteOrder.BIG_ENDIAN)
        header.put(MAGIC_BYTES)
        header.put(VERSION)
        header.put(CODEC_H264)
        header.putShort(FLAGS)
        header.putInt((current and 0xFFFFFFFFL).toInt())
        header.putLong(now)
        header.putShort(width.toShort())
        header.putShort(height.toShort())
        header.putInt(payload.size)
        try {
            out.write(header.array())
            out.write(payload)
            out.flush()
        } catch (e: IOException) {
            seq--
            throw e
        }
        frameBytes += payload.size + HEADER_LEN
        frameCount++
        if (windowStart == 0L) {
            windowStart = now
        }
        val elapsed = now - windowStart
        if (elapsed >= 2000L) {
            lastFps = ((frameCount * 1000L) / elapsed).toInt()
            lastKbps = (frameBytes * 8L) / (elapsed / 1000L.coerceAtLeast(1L)) / 1000L
            windowStart = now
            frameBytes = 0L
            frameCount = 0
        }
        _stats.value = MirrorStats(
            fps = lastFps,
            kbps = lastKbps,
            width = width,
            height = height,
            seq = current,
        )
    }

    private fun readLineRaw(input: InputStream, maxBytes: Int): String {
        val out = ByteArrayOutputStream()
        while (true) {
            val byte = input.read()
            if (byte == -1) {
                throw EOFException("mirror: peer closed during frame open")
            }
            if (byte == 0x0A) break
            if (byte != 0x0D) {
                out.write(byte)
                if (out.size() > maxBytes) {
                    throw IOException("mirror: frame open reply too long")
                }
            }
        }
        return out.toByteArray().toString(Charsets.UTF_8)
    }

    private fun isOk(body: kotlinx.serialization.json.JsonElement): Boolean {
        val obj = body as? JsonObject ?: return false
        val ok = obj["ok"] as? JsonPrimitive ?: return false
        return ok.content == "true"
    }

    private fun teardown(job: Job?) {
        try {
            job?.cancel()
        } catch (_: Exception) {
        }
        MirrorProcs.destroyAll()
        closeSocket()
        if (sessionJob === job) {
            sessionJob = null
        }
        sessionActive = false
        _videoSize.value = null
        _stats.value = null
        _state.value = MirrorLinkState.Idle
    }

    private fun closeSocket() {
        val socket = liveSocket ?: return
        liveSocket = null
        try {
            socket.close()
        } catch (_: Exception) {
        }
    }

    private fun scaledSize(maxW: Int): Pair<Int, Int> {
        val metrics = Resources.getSystem().displayMetrics
        val dw = metrics.widthPixels.coerceAtLeast(2)
        val dh = metrics.heightPixels.coerceAtLeast(2)
        val longest = maxOf(dw, dh)
        val scale = if (longest <= maxW || longest <= 0) {
            1.0
        } else {
            maxW.toDouble() / longest.toDouble()
        }
        val w = ((dw * scale).toInt() / 2 * 2).coerceAtLeast(2)
        val h = ((dh * scale).toInt() / 2 * 2).coerceAtLeast(2)
        return w to h
    }

    private fun openBody(fps: Int, maxW: Int) = buildJsonObject {
        put("t", JsonPrimitive("open"))
        put("fps", JsonPrimitive(fps))
        put("max_w", JsonPrimitive(maxW))
    }

    private fun releaseBody() = buildJsonObject {
        put("t", JsonPrimitive("release"))
    }

    private companion object {
        const val KIND_FRAME_OPEN = "nb.mirror.frame.open"
        const val HEADER_LEN = 28
        const val VERSION: Byte = 1
        const val CODEC_H264: Byte = 1
        const val FLAGS: Short = 0
        const val QUEUE_SLOTS = 64
        const val MAX_UNIT_BYTES = 8 * 1024 * 1024
        const val CONNECT_TIMEOUT_MS = 10000
        const val OPEN_TIMEOUT_MS = 10000
        const val CONTROL_TIMEOUT_MS = 10000L
        const val LINE_MAX_BYTES = 65536
        val MAGIC_BYTES = byteArrayOf(0x4E, 0x42, 0x4D, 0x31)
    }
}
