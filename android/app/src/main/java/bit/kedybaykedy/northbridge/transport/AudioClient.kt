package bit.kedybaykedy.northbridge.transport

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaCodec
import android.media.MediaFormat
import android.media.MediaRecorder
import android.os.SystemClock
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.EOFException
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicLong
import javax.net.ssl.SSLSocket
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
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
import kotlin.coroutines.coroutineContext

enum class AudioCaptureSource {
    MIC,
    SYSTEM,
}

enum class AudioSessionCodec {
    PCM,
    OPUS,
}

sealed interface AudioLinkState {
    data object Idle : AudioLinkState
    data object Connecting : AudioLinkState
    data object Streaming : AudioLinkState
    data class Error(val reason: String) : AudioLinkState
}

data class AudioStats(
    val sent: Long = 0L,
    val received: Long = 0L,
    val droppedSend: Long = 0L,
    val droppedReceive: Long = 0L,
)

private data class OutFrame(
    val seq: Long,
    val pts: Long,
    val codec: Int,
    val flags: Int,
    val payload: ByteArray,
)

class AudioClient(
    private val identity: PhoneIdentity,
    private val scope: CoroutineScope,
) {

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    private val _state = MutableStateFlow<AudioLinkState>(AudioLinkState.Idle)
    val state: StateFlow<AudioLinkState> = _state.asStateFlow()

    private val _stats = MutableStateFlow(AudioStats())
    val stats: StateFlow<AudioStats> = _stats.asStateFlow()

    @Volatile
    var micMuted = false

    @Volatile
    var speakerMuted = false

    @Volatile
    private var runJob: Job? = null

    @Volatile
    private var liveSocket: SSLSocket? = null

    @Volatile
    private var liveRecorder: AudioRecord? = null

    @Volatile
    private var sessionCodec = AudioSessionCodec.PCM

    @Volatile
    private var captureSource = AudioCaptureSource.MIC

    private val writeLock = Any()
    private val sentCount = AtomicLong(0L)
    private val receivedCount = AtomicLong(0L)
    private val sentDropCount = AtomicLong(0L)
    private val recvDropCount = AtomicLong(0L)

    fun start(
        host: String,
        audioPort: Int,
        fingerprint: String,
        codec: AudioSessionCodec,
        source: AudioCaptureSource,
    ) {
        stop()
        if (fingerprint.isBlank() || audioPort !in 1..65535) {
            _state.value = AudioLinkState.Error("no paired device")
            return
        }
        sessionCodec = codec
        captureSource = source
        sentCount.set(0L)
        receivedCount.set(0L)
        sentDropCount.set(0L)
        recvDropCount.set(0L)
        _stats.value = AudioStats()
        runJob = scope.launch { runLoop(host, audioPort, fingerprint) }
    }

    fun stop() {
        runJob?.cancel()
        runJob = null
        try {
            liveRecorder?.stop()
        } catch (_: Exception) {
        }
        closeSocket()
        _state.value = AudioLinkState.Idle
    }

    private suspend fun runLoop(host: String, audioPort: Int, fingerprint: String) {
        var attempt = 0
        try {
            while (true) {
                _state.value = AudioLinkState.Connecting
                try {
                    withContext(Dispatchers.IO) {
                        runSession(host, audioPort, fingerprint)
                    }
                    attempt = 0
                } catch (cancellation: CancellationException) {
                    throw cancellation
                } catch (_: SecurityException) {
                    _state.value = AudioLinkState.Error("system capture needs root grant")
                    return
                } catch (_: UnsupportedOperationException) {
                    _state.value = AudioLinkState.Error("capture unsupported on device")
                    return
                } catch (_: Exception) {
                }
                val backoff = (500L shl attempt.coerceAtMost(4)).coerceAtMost(8000L)
                attempt++
                delay(backoff)
            }
        } catch (cancellation: CancellationException) {
            _state.value = AudioLinkState.Idle
            throw cancellation
        }
    }

    private suspend fun runSession(host: String, audioPort: Int, fingerprint: String) {
        val trust = Tls.pinned(fingerprint)
        val factory = Tls.socketFactory(identity, trust)
        val socket = factory.createSocket() as SSLSocket
        var recorder: AudioRecord? = null
        var track: AudioTrack? = null
        var encoder: OpusBridge? = null
        var decoder: OpusBridge? = null
        try {
            socket.connect(InetSocketAddress(host, audioPort), CONNECT_TIMEOUT_MS)
            socket.soTimeout = OPEN_TIMEOUT_MS
            socket.keepAlive = true
            socket.tcpNoDelay = true
            try {
                socket.sendBufferSize = 32 * 1024
            } catch (_: Exception) {
            }
            socket.startHandshake()
            writeOpenLine(socket)
            awaitOpenAck(socket)
            socket.soTimeout = 0
            liveSocket = socket
            _state.value = AudioLinkState.Streaming
            val wantOpus = sessionCodec == AudioSessionCodec.OPUS
            if (wantOpus) {
                val enc = OpusBridge()
                if (enc.startEncoder()) {
                    encoder = enc
                } else {
                    enc.release()
                }
            }
            if (wantOpus) {
                val dec = OpusBridge()
                if (dec.startDecoder()) {
                    decoder = dec
                } else {
                    dec.release()
                }
            }
            val built = buildRecorder()
            recorder = built.first
            liveRecorder = recorder
            val stereoIn = built.second
            track = buildTrack()
            val queue = Channel<OutFrame>(
                capacity = 3,
                onBufferOverflow = BufferOverflow.DROP_OLDEST,
            )
            val out = socket.outputStream
            val input = DataInputStream(socket.inputStream)
            coroutineScope {
                launch { captureLoop(recorder, stereoIn, encoder, queue, wantOpus) }
                launch { writeLoop(out, queue) }
                receiveLoop(input, track, decoder)
            }
        } finally {
            if (liveSocket === socket) {
                liveSocket = null
            }
            if (liveRecorder === recorder) {
                liveRecorder = null
            }
            try {
                encoder?.release()
            } catch (_: Exception) {
            }
            try {
                decoder?.release()
            } catch (_: Exception) {
            }
            try {
                recorder?.release()
            } catch (_: Exception) {
            }
            try {
                track?.release()
            } catch (_: Exception) {
            }
            try {
                socket.close()
            } catch (_: Exception) {
            }
        }
    }

    private fun writeOpenLine(socket: SSLSocket) {
        val line = json.encodeToString(
            NetworkPacket(
                id = System.currentTimeMillis(),
                kind = KIND_AUDIO_OPEN,
                body = JsonObject(emptyMap()),
            ),
        ) + "\n"
        synchronized(writeLock) {
            socket.outputStream.write(line.toByteArray(Charsets.UTF_8))
            socket.outputStream.flush()
        }
    }

    private fun awaitOpenAck(socket: SSLSocket) {
        while (true) {
            val line = readLineRaw(socket.inputStream, LINE_MAX_BYTES)
            val packet = try {
                json.decodeFromString<NetworkPacket>(line)
            } catch (_: SerializationException) {
                continue
            } catch (_: IllegalArgumentException) {
                continue
            }
            if (packet.kind != KIND_AUDIO_OPEN) {
                continue
            }
            if (!isOk(packet.body)) {
                throw IOException("audio: open rejected")
            }
            return
        }
    }

    private suspend fun captureLoop(
        recorder: AudioRecord,
        stereoIn: Boolean,
        encoder: OpusBridge?,
        queue: Channel<OutFrame>,
        wantOpus: Boolean,
    ) {
        val shortsPerRead = if (stereoIn) FRAME_SAMPLES * CHANNELS else FRAME_SAMPLES
        val shorts = ShortArray(shortsPerRead)
        var seq = 0L
        recorder.startRecording()
        try {
            while (true) {
                var got = 0
                while (got < shortsPerRead) {
                    val n = recorder.read(shorts, got, shortsPerRead - got)
                    if (n < 0) {
                        throw IOException("audio: record error " + n)
                    }
                    if (n == 0) {
                        continue
                    }
                    got += n
                }
                coroutineContext.ensureActive()
                val pcm = if (stereoIn) {
                    shortsToBytes(shorts)
                } else {
                    shortsToBytes(monoToStereo(shorts))
                }
                val muted = micMuted
                val payload: ByteArray
                val codecByte: Int
                if (muted) {
                    payload = ByteArray(0)
                    codecByte = CODEC_PCM
                } else if (wantOpus && encoder != null) {
                    val opus = try {
                        encoder.encode(pcm)
                    } catch (_: Exception) {
                        null
                    }
                    if (opus != null) {
                        payload = opus
                        codecByte = CODEC_OPUS
                    } else {
                        payload = pcm
                        codecByte = CODEC_PCM
                    }
                } else {
                    payload = pcm
                    codecByte = CODEC_PCM
                }
                var flags = (1 shl FLAG_DIR)
                if (muted) {
                    flags = flags or (1 shl FLAG_DTX)
                }
                queue.trySend(OutFrame(seq, SystemClock.elapsedRealtime(), codecByte, flags, payload))
                seq++
            }
        } finally {
            try {
                recorder.stop()
            } catch (_: Exception) {
            }
        }
    }

    private suspend fun writeLoop(out: OutputStream, queue: Channel<OutFrame>) {
        var expected = 0L
        var first = true
        for (frame in queue) {
            if (!first && frame.seq != expected) {
                sentDropCount.addAndGet(frame.seq - expected)
            }
            first = false
            expected = frame.seq + 1
            val header = ByteBuffer.allocate(HEADER_LEN).order(ByteOrder.LITTLE_ENDIAN)
            header.putInt(MAGIC_NBA1)
            header.put(VERSION)
            header.put(frame.codec.toByte())
            header.put(frame.flags.toByte())
            header.put(CHANNELS.toByte())
            header.putInt((frame.seq and 0xFFFFFFFFL).toInt())
            header.putLong(frame.pts)
            header.putInt(frame.payload.size)
            withContext(Dispatchers.IO) {
                synchronized(writeLock) {
                    out.write(header.array())
                    if (frame.payload.isNotEmpty()) {
                        out.write(frame.payload)
                    }
                    out.flush()
                }
            }
            sentCount.incrementAndGet()
            publishStats()
        }
    }

    private suspend fun receiveLoop(input: DataInputStream, track: AudioTrack, decoder: OpusBridge?) {
        val header = ByteArray(HEADER_LEN)
        var lastSeq = -1L
        track.play()
        try {
            while (true) {
                withContext(Dispatchers.IO) {
                    input.readFully(header)
                }
                coroutineContext.ensureActive()
                val buf = ByteBuffer.wrap(header).order(ByteOrder.LITTLE_ENDIAN)
                val magic = buf.int
                val version = buf.get().toInt() and 0xFF
                val codec = buf.get().toInt() and 0xFF
                val flags = buf.get().toInt() and 0xFF
                val channels = buf.get().toInt() and 0xFF
                val seq = buf.int.toLong() and 0xFFFFFFFFL
                buf.long
                val len = buf.int
                if (magic != MAGIC_NBA1 || version != VERSION.toInt() || len < 0 || len > MAX_FRAME_BYTES ||
                    (channels != 1 && channels != 2) || (codec != CODEC_OPUS && codec != CODEC_PCM)
                ) {
                    throw IOException("audio: bad header")
                }
                val payload = if (len > 0) {
                    val data = ByteArray(len)
                    withContext(Dispatchers.IO) {
                        input.readFully(data)
                    }
                    data
                } else {
                    ByteArray(0)
                }
                val fromPhone = (flags and (1 shl FLAG_DIR)) != 0
                if (fromPhone) {
                    recvDropCount.incrementAndGet()
                    publishStats()
                    continue
                }
                if (lastSeq >= 0 && seq <= lastSeq) {
                    recvDropCount.incrementAndGet()
                    publishStats()
                    continue
                }
                if (lastSeq >= 0 && seq > lastSeq + 1) {
                    recvDropCount.addAndGet(seq - lastSeq - 1)
                }
                lastSeq = seq
                if (speakerMuted || len == 0) {
                    receivedCount.incrementAndGet()
                    publishStats()
                    continue
                }
                val pcm = if (codec == CODEC_PCM) {
                    payload
                } else {
                    try {
                        decoder?.decode(payload)
                    } catch (_: Exception) {
                        null
                    }
                }
                if (pcm == null) {
                    recvDropCount.incrementAndGet()
                    publishStats()
                    continue
                }
                val stereo = if (channels == 1) monoBytesToStereo(pcm) else pcm
                withContext(Dispatchers.IO) {
                    track.write(stereo, 0, stereo.size)
                }
                receivedCount.incrementAndGet()
                publishStats()
            }
        } finally {
            try {
                track.stop()
            } catch (_: Exception) {
            }
        }
    }

    private fun buildRecorder(): Pair<AudioRecord, Boolean> {
        val audioSource = if (captureSource == AudioCaptureSource.SYSTEM) {
            MediaRecorder.AudioSource.REMOTE_SUBMIX
        } else {
            MediaRecorder.AudioSource.VOICE_COMMUNICATION
        }
        val minStereo = try {
            AudioRecord.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_STEREO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
        } catch (_: Exception) {
            AudioRecord.ERROR_BAD_VALUE
        }
        if (minStereo > 0) {
            val candidate = AudioRecord.Builder()
                .setAudioSource(audioSource)
                .setAudioFormat(
                    AudioFormat.Builder()
                        .setSampleRate(SAMPLE_RATE)
                        .setChannelMask(AudioFormat.CHANNEL_IN_STEREO)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .build(),
                )
                .setBufferSizeInBytes(maxOf(minStereo, FRAME_BYTES * 4))
                .build()
            if (candidate.state == AudioRecord.STATE_INITIALIZED) {
                return Pair(candidate, true)
            }
            try {
                candidate.release()
            } catch (_: Exception) {
            }
        }
        val minMono = try {
            AudioRecord.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
        } catch (_: Exception) {
            AudioRecord.ERROR_BAD_VALUE
        }
        if (minMono <= 0) {
            throw IOException("audio: no recorder available")
        }
        val fallback = AudioRecord.Builder()
            .setAudioSource(audioSource)
            .setAudioFormat(
                AudioFormat.Builder()
                    .setSampleRate(SAMPLE_RATE)
                    .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .build(),
            )
            .setBufferSizeInBytes(maxOf(minMono, FRAME_BYTES * 2))
            .build()
        if (fallback.state != AudioRecord.STATE_INITIALIZED) {
            try {
                fallback.release()
            } catch (_: Exception) {
            }
            throw IOException("audio: recorder init failed")
        }
        return Pair(fallback, false)
    }

    private fun buildTrack(): AudioTrack {
        val min = try {
            AudioTrack.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_OUT_STEREO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
        } catch (_: Exception) {
            0
        }
        val size = maxOf(if (min > 0) min else 0, FRAME_BYTES * 3)
        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
            .build()
        val format = AudioFormat.Builder()
            .setSampleRate(SAMPLE_RATE)
            .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .build()
        return try {
            AudioTrack.Builder()
                .setAudioAttributes(attributes)
                .setAudioFormat(format)
                .setBufferSizeInBytes(size)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                .build()
        } catch (_: Exception) {
            AudioTrack.Builder()
                .setAudioAttributes(attributes)
                .setAudioFormat(format)
                .setBufferSizeInBytes(size)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()
        }
    }

    private fun readLineRaw(input: InputStream, maxBytes: Int): String {
        val out = ByteArrayOutputStream()
        while (true) {
            val byte = input.read()
            if (byte == -1) {
                throw EOFException("audio: peer closed during open")
            }
            if (byte == 0x0A) {
                break
            }
            if (byte != 0x0D) {
                out.write(byte)
                if (out.size() > maxBytes) {
                    throw IOException("audio: open reply too long")
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

    private fun publishStats() {
        _stats.value = AudioStats(
            sent = sentCount.get(),
            received = receivedCount.get(),
            droppedSend = sentDropCount.get(),
            droppedReceive = recvDropCount.get(),
        )
    }

    private fun closeSocket() {
        val socket = liveSocket ?: return
        liveSocket = null
        try {
            socket.close()
        } catch (_: Exception) {
        }
    }

    private fun shortsToBytes(shorts: ShortArray): ByteArray {
        val out = ByteBuffer.allocate(shorts.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        for (sample in shorts) {
            out.putShort(sample)
        }
        return out.array()
    }

    private fun monoToStereo(mono: ShortArray): ShortArray {
        val stereo = ShortArray(mono.size * 2)
        for (i in mono.indices) {
            stereo[i * 2] = mono[i]
            stereo[i * 2 + 1] = mono[i]
        }
        return stereo
    }

    private fun monoBytesToStereo(mono: ByteArray): ByteArray {
        val frames = mono.size / 2
        val out = ByteBuffer.allocate(frames * 4).order(ByteOrder.LITTLE_ENDIAN)
        val view = ByteBuffer.wrap(mono).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        var i = 0
        while (i < frames) {
            val sample = if (view.hasRemaining()) view.get() else 0
            out.putShort(sample)
            out.putShort(sample)
            i++
        }
        return out.array()
    }

    private companion object {
        const val KIND_AUDIO_OPEN = "nb.audio.open"
        const val HEADER_LEN = 24
        const val MAGIC_NBA1 = 0x3141424E
        const val VERSION: Byte = 1
        const val CODEC_OPUS = 0
        const val CODEC_PCM = 1
        const val FLAG_DIR = 1
        const val FLAG_DTX = 2
        const val CHANNELS = 2
        const val SAMPLE_RATE = 48000
        const val FRAME_SAMPLES = 960
        const val FRAME_BYTES = 3840
        const val MAX_FRAME_BYTES = 1024 * 1024
        const val CONNECT_TIMEOUT_MS = 5000
        const val OPEN_TIMEOUT_MS = 10000
        const val LINE_MAX_BYTES = 65536
    }
}

private class OpusBridge {

    private var codec: MediaCodec? = null
    private var encodePts = 0L

    fun startEncoder(): Boolean {
        return try {
            val active = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, 48000, 2)
            format.setInteger(MediaFormat.KEY_BIT_RATE, 64000)
            format.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 8192)
            active.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            active.start()
            codec = active
            true
        } catch (_: Exception) {
            false
        }
    }

    fun startDecoder(): Boolean {
        return try {
            val active = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, 48000, 2)
            format.setByteBuffer("csd-0", ByteBuffer.wrap(opusHead()))
            format.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 8192)
            active.configure(format, null, null, 0)
            active.start()
            codec = active
            true
        } catch (_: Exception) {
            false
        }
    }

    fun encode(pcm: ByteArray): ByteArray? {
        val active = codec ?: return null
        val slot = active.dequeueInputBuffer(10000)
        if (slot < 0) {
            return null
        }
        val room = active.getInputBuffer(slot) ?: return null
        room.clear()
        if (room.remaining() < pcm.size) {
            active.queueInputBuffer(slot, 0, 0, 0L, 0)
            return null
        }
        room.put(pcm)
        encodePts += 20000L
        active.queueInputBuffer(slot, 0, pcm.size, encodePts, 0)
        val info = MediaCodec.BufferInfo()
        val done = active.dequeueOutputBuffer(info, 10000)
        if (done < 0) {
            return null
        }
        val view = active.getOutputBuffer(done)
        val data = if (view != null && info.size > 0) {
            val copy = ByteArray(info.size)
            view.position(info.offset)
            view.limit(info.offset + info.size)
            view.get(copy)
            copy
        } else {
            null
        }
        active.releaseOutputBuffer(done, false)
        return data
    }

    fun decode(opus: ByteArray): ByteArray? {
        val active = codec ?: return null
        val slot = active.dequeueInputBuffer(10000)
        if (slot < 0) {
            return null
        }
        val room = active.getInputBuffer(slot) ?: return null
        room.clear()
        if (room.remaining() < opus.size) {
            active.queueInputBuffer(slot, 0, 0, 0L, 0)
            return null
        }
        room.put(opus)
        encodePts += 20000L
        active.queueInputBuffer(slot, 0, opus.size, encodePts, 0)
        val collected = ByteArrayOutputStream()
        val info = MediaCodec.BufferInfo()
        var slot2 = active.dequeueOutputBuffer(info, 10000)
        if (slot2 == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
            slot2 = active.dequeueOutputBuffer(info, 0)
        }
        var guard = 0
        var got = false
        while (slot2 >= 0 && guard < 8) {
            val view = active.getOutputBuffer(slot2)
            if (view != null && info.size > 0) {
                val copy = ByteArray(info.size)
                view.position(info.offset)
                view.limit(info.offset + info.size)
                view.get(copy)
                collected.write(copy)
                got = true
            }
            active.releaseOutputBuffer(slot2, false)
            slot2 = active.dequeueOutputBuffer(info, 0)
            guard++
        }
        return if (got) collected.toByteArray() else null
    }

    fun release() {
        val active = codec ?: return
        codec = null
        try {
            active.stop()
        } catch (_: Exception) {
        }
        try {
            active.release()
        } catch (_: Exception) {
        }
    }

    private fun opusHead(): ByteArray {
        val head = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
        head.put("OpusHead".toByteArray(Charsets.US_ASCII))
        head.put(1)
        head.put(2)
        head.putShort(384)
        head.putInt(48000)
        head.putShort(0)
        head.put(0)
        return head.array()
    }
}
