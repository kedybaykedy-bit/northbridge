package bit.kedybaykedy.northbridge.transport

import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.EOFException
import java.io.IOException
import java.io.InputStream
import java.net.InetSocketAddress
import java.nio.ByteBuffer
import java.nio.ByteOrder
import javax.net.ssl.SSLSocket
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

data class ScreenFrame(
    val image: ImageBitmap,
    val seq: Long,
    val width: Int,
    val height: Int,
)

sealed interface ScreenLinkState {
    data object Idle : ScreenLinkState
    data object Connecting : ScreenLinkState
    data object Streaming : ScreenLinkState
    data class Error(val reason: String) : ScreenLinkState
}

class ScreenClient(
    private val identity: PhoneIdentity,
    private val scope: CoroutineScope,
) {

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    private val _state = MutableStateFlow<ScreenLinkState>(ScreenLinkState.Idle)
    val state: StateFlow<ScreenLinkState> = _state.asStateFlow()

    private val _frames = MutableSharedFlow<ScreenFrame>(
        extraBufferCapacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val frames: SharedFlow<ScreenFrame> = _frames.asSharedFlow()

    @Volatile
    var droppedFrames = 0L
        private set

    @Volatile
    private var runJob: Job? = null

    @Volatile
    private var liveSocket: SSLSocket? = null

    fun start(host: String, framePort: Int, fingerprint: String) {
        stop()
        if (fingerprint.isBlank() || framePort !in 1..65535) {
            _state.value = ScreenLinkState.Error("no paired device")
            return
        }
        droppedFrames = 0L
        runJob = scope.launch { runLoop(host, framePort, fingerprint) }
    }

    fun stop() {
        runJob?.cancel()
        runJob = null
        closeSocket()
        _state.value = ScreenLinkState.Idle
    }

    private suspend fun runLoop(host: String, framePort: Int, fingerprint: String) {
        var attempt = 0
        try {
            while (true) {
                _state.value = ScreenLinkState.Connecting
                try {
                    withContext(Dispatchers.IO) {
                        runSession(host, framePort, fingerprint)
                    }
                    attempt = 0
                } catch (cancellation: CancellationException) {
                    throw cancellation
                } catch (_: Exception) {
                }
                val backoff = (500L shl attempt.coerceAtMost(4)).coerceAtMost(8000L)
                attempt++
                delay(backoff)
            }
        } catch (cancellation: CancellationException) {
            _state.value = ScreenLinkState.Idle
            throw cancellation
        }
    }

    private fun runSession(host: String, framePort: Int, fingerprint: String) {
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
                throw IOException("screen: malformed open reply")
            } catch (_: IllegalArgumentException) {
                throw IOException("screen: malformed open reply")
            }
            if (reply.kind != KIND_FRAME_OPEN || !isOk(reply.body)) {
                throw IOException("screen: open rejected")
            }
            socket.soTimeout = FRAME_TIMEOUT_MS
            liveSocket = socket
            _state.value = ScreenLinkState.Streaming
            readFrames(DataInputStream(socket.inputStream))
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

    private fun readFrames(input: DataInputStream) {
        val header = ByteArray(HEADER_LEN)
        var lastSeq = -1L
        while (true) {
            input.readFully(header)
            val buf = ByteBuffer.wrap(header).order(ByteOrder.BIG_ENDIAN)
            if (buf.int != MAGIC_NBS1) {
                throw IOException("screen: bad magic")
            }
            val version = buf.get().toInt() and 0xFF
            val codec = buf.get().toInt() and 0xFF
            buf.short
            val seq = buf.int.toLong() and 0xFFFFFFFFL
            buf.long
            val width = buf.short.toInt() and 0xFFFF
            val height = buf.short.toInt() and 0xFFFF
            val length = buf.int
            if (version != 1 || codec != 0 || width <= 0 || height <= 0 ||
                length <= 0 || length > MAX_FRAME_BYTES
            ) {
                throw IOException("screen: bad header")
            }
            val payload = ByteArray(length)
            input.readFully(payload)
            if (lastSeq >= 0 && seq > lastSeq + 1) {
                droppedFrames += seq - lastSeq - 1
            }
            lastSeq = seq
            val frame = decodeFrame(payload, width, height, seq) ?: continue
            _frames.tryEmit(frame)
        }
    }

    private fun decodeFrame(jpeg: ByteArray, width: Int, height: Int, seq: Long): ScreenFrame? {
        val pixels = width.toLong() * height.toLong()
        var sample = 1
        while (pixels / (sample.toLong() * sample.toLong()) > MAX_PIXELS) {
            sample *= 2
        }
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        val bitmap = BitmapFactory.decodeByteArray(jpeg, 0, jpeg.size, options) ?: return null
        return ScreenFrame(
            image = bitmap.asImageBitmap(),
            seq = seq,
            width = width,
            height = height,
        )
    }

    private fun readLineRaw(input: InputStream, maxBytes: Int): String {
        val out = ByteArrayOutputStream()
        while (true) {
            val byte = input.read()
            if (byte == -1) {
                throw EOFException("screen: peer closed during open")
            }
            if (byte == 0x0A) {
                break
            }
            if (byte != 0x0D) {
                out.write(byte)
                if (out.size() > maxBytes) {
                    throw IOException("screen: open reply too long")
                }
            }
        }
        return out.toByteArray().toString(Charsets.UTF_8)
    }

    private fun isOk(body: JsonElement): Boolean {
        val obj = body as? JsonObject ?: return false
        val ok = obj["ok"] as? JsonPrimitive ?: return false
        return ok.content == "true"
    }

    private fun closeSocket() {
        val socket = liveSocket ?: return
        liveSocket = null
        try {
            socket.close()
        } catch (_: Exception) {
        }
    }

    private companion object {
        const val KIND_FRAME_OPEN = "nb.screen.frame.open"
        const val HEADER_LEN = 28
        const val MAGIC_NBS1 = 0x4E425331
        const val MAX_FRAME_BYTES = 8 * 1024 * 1024
        const val MAX_PIXELS = 1_000_000L
        const val CONNECT_TIMEOUT_MS = 5000
        const val OPEN_TIMEOUT_MS = 10000
        const val FRAME_TIMEOUT_MS = 30000
        const val LINE_MAX_BYTES = 65536
    }
}
