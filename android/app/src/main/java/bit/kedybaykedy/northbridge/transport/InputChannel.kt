package bit.kedybaykedy.northbridge.transport

import bit.kedybaykedy.northbridge.data.TrustedDevice
import java.io.BufferedReader
import java.io.IOException
import java.io.InputStreamReader
import java.net.InetSocketAddress
import javax.net.ssl.SSLSocket
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
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
import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlin.coroutines.coroutineContext

enum class ConnectionState {
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
    RECONNECTING,
}

class InputChannel(
    private val identity: PhoneIdentity,
    private val scope: CoroutineScope,
) {

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        classDiscriminator = "t"
    }

    private val _state = MutableStateFlow(ConnectionState.DISCONNECTED)
    val state: StateFlow<ConnectionState> = _state.asStateFlow()

    private val _messages = MutableSharedFlow<InputMsg>(
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val messages: SharedFlow<InputMsg> = _messages.asSharedFlow()

    private val outbox = Channel<InputMsg>(Channel.UNLIMITED)
    private val writeLock = Any()
    private var connectionJob: Job? = null

    @Volatile
    private var generation = 0

    @Volatile
    private var activeSocket: SSLSocket? = null

    @Volatile
    private var attached: TrustedDevice? = null

    @Volatile
    private var lastActivityAt = 0L

    fun attach(target: TrustedDevice?) {
        generation++
        attached = target
        closeActiveSocket()
        connectionJob?.cancel()
        connectionJob = null
        drainOutbox()
        if (target == null) {
            setState(generation, ConnectionState.DISCONNECTED)
            return
        }
        val ownGeneration = generation
        setState(ownGeneration, ConnectionState.CONNECTING)
        connectionJob = scope.launch { runConnection(target, ownGeneration) }
    }

    fun send(msg: InputMsg) {
        if (attached == null) return
        outbox.trySend(msg)
    }

    private fun setState(ownGeneration: Int, state: ConnectionState) {
        if (ownGeneration == generation) {
            _state.value = state
        }
    }

    private suspend fun runConnection(target: TrustedDevice, ownGeneration: Int) {
        var attempt = 0
        try {
            while (true) {
                setState(
                    ownGeneration,
                    if (attempt == 0) ConnectionState.CONNECTING else ConnectionState.RECONNECTING,
                )
                purgeStaleMoves()
                var socket: SSLSocket? = null
                try {
                    val connected = connect(target)
                    coroutineContext.ensureActive()
                    socket = connected
                    activeSocket = connected
                    coroutineContext.ensureActive()
                    val reader = BufferedReader(InputStreamReader(connected.inputStream, Charsets.UTF_8))
                    handshake(connected, reader)
                    coroutineContext.ensureActive()
                    setState(ownGeneration, ConnectionState.CONNECTED)
                    lastActivityAt = System.currentTimeMillis()
                    attempt = 0
                    coroutineScope {
                        launch { writeLoop() }
                        launch { keepaliveLoop() }
                        readLoop(reader)
                    }
                } catch (cancellation: CancellationException) {
                    throw cancellation
                } catch (_: Exception) {
                } finally {
                    val closing = socket
                    socket = null
                    if (closing != null) {
                        if (activeSocket === closing) {
                            activeSocket = null
                        }
                        try {
                            closing.close()
                        } catch (_: Exception) {
                        }
                    }
                }
                setState(ownGeneration, ConnectionState.RECONNECTING)
                val backoff = (250L shl attempt.coerceAtMost(5)).coerceAtMost(8000L)
                attempt++
                delay(backoff)
            }
        } catch (cancellation: CancellationException) {
            setState(ownGeneration, ConnectionState.DISCONNECTED)
            throw cancellation
        }
    }

    private fun connect(target: TrustedDevice): SSLSocket {
        val trust = Tls.pinned(target.fingerprint)
        val factory = Tls.socketFactory(identity, trust)
        val socket = factory.createSocket() as SSLSocket
        try {
            socket.connect(InetSocketAddress(target.host, target.packetPort), 5000)
            socket.soTimeout = 0
            socket.startHandshake()
        } catch (e: Exception) {
            try {
                socket.close()
            } catch (_: Exception) {
            }
            throw e
        }
        return socket
    }

    private fun handshake(socket: SSLSocket, reader: BufferedReader) {
        writePacket(
            NetworkPacket(
                id = System.currentTimeMillis(),
                kind = KIND_OPEN,
                body = JsonObject(emptyMap()),
            ),
        )
        socket.soTimeout = OPEN_ACK_TIMEOUT_MS
        try {
            while (true) {
                val line = reader.readLine() ?: throw IOException("peer closed before open ack")
                val packet = parsePacket(line) ?: continue
                if (packet.kind != KIND_OPEN) continue
                if (!isAccepted(packet.body)) throw IOException("stream open rejected")
                return
            }
        } finally {
            socket.soTimeout = 0
        }
    }

    private fun parsePacket(line: String): NetworkPacket? = try {
        json.decodeFromString<NetworkPacket>(line)
    } catch (_: SerializationException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    private fun isAccepted(body: JsonElement): Boolean {
        val obj = body as? JsonObject ?: return false
        val ok = obj["ok"] as? JsonPrimitive ?: return false
        return ok.content == "true"
    }

    private suspend fun writeLoop() {
        for (msg in outbox) {
            try {
                writeLine(msg)
            } catch (failure: IOException) {
                outbox.trySend(msg)
                throw failure
            }
        }
    }

    private suspend fun keepaliveLoop() {
        while (true) {
            delay(KEEPALIVE_INTERVAL_MS)
            if (System.currentTimeMillis() - lastActivityAt > KEEPALIVE_TIMEOUT_MS) {
                throw IOException("keepalive timed out")
            }
            writeLine(InputMsg.Ping)
        }
    }

    private suspend fun readLoop(reader: BufferedReader) {
        while (true) {
            val line = reader.readLine() ?: throw IOException("connection closed by peer")
            lastActivityAt = System.currentTimeMillis()
            val packet = parsePacket(line) ?: continue
            when (packet.kind) {
                KIND_KVM -> {
                    val msg = decodeBody(packet.body) ?: continue
                    when (msg) {
                        is InputMsg.Enter, is InputMsg.Move, InputMsg.Leave -> _messages.emit(msg)
                        else -> Unit
                    }
                }
                KIND_TOUCHPAD -> {
                    if (decodeBody(packet.body) is InputMsg.Pong) {
                        lastActivityAt = System.currentTimeMillis()
                    }
                }
                KIND_OPEN -> Unit
                KIND_ERROR -> Unit
                else -> Unit
            }
        }
    }

    private fun decodeBody(body: JsonElement): InputMsg? = try {
        json.decodeFromJsonElement(InputMsg.serializer(), body)
    } catch (_: SerializationException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    private fun writeLine(msg: InputMsg) {
        writePacket(
            NetworkPacket(
                id = System.currentTimeMillis(),
                kind = KIND_TOUCHPAD,
                body = json.encodeToJsonElement(InputMsg.serializer(), msg),
            ),
        )
    }

    private fun writePacket(packet: NetworkPacket) {
        val socket = activeSocket ?: throw IOException("not connected")
        val payload = json.encodeToString(packet) + "\n"
        synchronized(writeLock) {
            socket.outputStream.write(payload.toByteArray(Charsets.UTF_8))
            socket.outputStream.flush()
        }
        lastActivityAt = System.currentTimeMillis()
    }

    private fun closeActiveSocket() {
        val socket = activeSocket ?: return
        activeSocket = null
        try {
            socket.close()
        } catch (_: Exception) {
        }
    }

    private fun purgeStaleMoves() {
        val keep = ArrayList<InputMsg>(OUTBOX_KEEP)
        while (true) {
            val msg = outbox.tryReceive().getOrNull() ?: break
            if (msg !is InputMsg.RelMove && msg !is InputMsg.Scroll) {
                keep.add(msg)
            }
        }
        if (keep.size > OUTBOX_KEEP) {
            keep.subList(0, keep.size - OUTBOX_KEEP).clear()
        }
        for (msg in keep) {
            outbox.trySend(msg)
        }
    }

    private fun drainOutbox() {
        while (outbox.tryReceive().isSuccess) {
        }
    }

    private companion object {
        const val OUTBOX_KEEP = 256
        const val KEEPALIVE_INTERVAL_MS = 5000L
        const val KEEPALIVE_TIMEOUT_MS = 15000L
        const val OPEN_ACK_TIMEOUT_MS = 5000
        const val KIND_OPEN = "nb.touchpad.open"
        const val KIND_TOUCHPAD = "nb.touchpad"
        const val KIND_KVM = "nb.kvm"
        const val KIND_ERROR = "nb.error"
    }
}
