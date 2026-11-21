package bit.kedybaykedy.northbridge.ui.audio

import androidx.lifecycle.ViewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.TrustedDevice
import bit.kedybaykedy.northbridge.transport.AudioCaptureSource
import bit.kedybaykedy.northbridge.transport.AudioClient
import bit.kedybaykedy.northbridge.transport.AudioLinkState
import bit.kedybaykedy.northbridge.transport.AudioSessionCodec
import bit.kedybaykedy.northbridge.transport.AudioStats
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

enum class AudioUiState {
    IDLE,
    OPENING,
    ACTIVE,
    CLOSED,
    ERROR,
}

enum class AudioSourceOption {
    MIC,
    SYSTEM,
}

class AudioViewModel : ViewModel() {

    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client = AudioClient(AppGraph.identity, ioScope)

    private val _uiState = MutableStateFlow(AudioUiState.IDLE)
    val uiState: StateFlow<AudioUiState> = _uiState.asStateFlow()

    private val _meta = MutableStateFlow("")
    val meta: StateFlow<String> = _meta.asStateFlow()

    private val _deviceAlias = MutableStateFlow("")
    val deviceAlias: StateFlow<String> = _deviceAlias.asStateFlow()

    private val _codecLabel = MutableStateFlow("PCM")
    val codecLabel: StateFlow<String> = _codecLabel.asStateFlow()

    private val _source = MutableStateFlow(AudioSourceOption.MIC)
    val source: StateFlow<AudioSourceOption> = _source.asStateFlow()

    private val _micMuted = MutableStateFlow(false)
    val micMuted: StateFlow<Boolean> = _micMuted.asStateFlow()

    private val _speakerMuted = MutableStateFlow(false)
    val speakerMuted: StateFlow<Boolean> = _speakerMuted.asStateFlow()

    private val _sent = MutableStateFlow(0L)
    val sent: StateFlow<Long> = _sent.asStateFlow()

    private val _received = MutableStateFlow(0L)
    val received: StateFlow<Long> = _received.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    @Volatile
    private var sessionActive = false

    private var controlJob: Job? = null
    private var linkJob: Job? = null
    private var statsJob: Job? = null

    fun start() {
        if (sessionActive) {
            return
        }
        sessionActive = true
        ioScope.launch { openSession() }
    }

    fun stop() {
        ioScope.launch {
            try {
                AppGraph.inputChannel.sendPacket(KIND_AUDIO, releaseBody())
            } catch (_: Exception) {
            }
        }
        teardown(AudioUiState.CLOSED)
    }

    fun setSource(option: AudioSourceOption) {
        if (sessionActive) {
            return
        }
        _source.value = option
    }

    fun toggleMicMute() {
        _micMuted.value = !_micMuted.value
        client.micMuted = _micMuted.value
    }

    fun toggleSpeakerMute() {
        _speakerMuted.value = !_speakerMuted.value
        client.speakerMuted = _speakerMuted.value
    }

    override fun onCleared() {
        teardown(AudioUiState.IDLE)
        ioScope.cancel()
    }

    private suspend fun openSession() {
        _error.value = null
        _meta.value = ""
        _sent.value = 0L
        _received.value = 0L
        val device = activeDevice()
        if (device == null) {
            _error.value = "Önce Cihazlar sekmesinde bir masaüstü eşleştir"
            _uiState.value = AudioUiState.IDLE
            sessionActive = false
            return
        }
        _deviceAlias.value = device.alias
        _uiState.value = AudioUiState.OPENING
        val opened = CompletableDeferred<AudioOpen?>()
        controlJob?.cancel()
        controlJob = ioScope.launch {
            AppGraph.inputChannel.controlPackets.collect { packet ->
                if (packet.kind != KIND_AUDIO) {
                    return@collect
                }
                val body = packet.body as? JsonObject ?: return@collect
                when ((body["t"] as? JsonPrimitive)?.content) {
                    "opened" -> {
                        if (!opened.isCompleted) {
                            val port = (body["port"] as? JsonPrimitive)?.content?.toIntOrNull()
                                ?: (device.packetPort + 2)
                            val codec = (body["codec"] as? JsonPrimitive)?.content?.lowercase()
                            opened.complete(AudioOpen(port, codec == "opus"))
                        }
                    }
                    "closed" -> {
                        if (!opened.isCompleted) {
                            opened.complete(null)
                        } else {
                            teardown(AudioUiState.CLOSED)
                        }
                    }
                }
            }
        }
        val sent = AppGraph.inputChannel.sendPacket(KIND_AUDIO, openBody())
        if (!sent) {
            _error.value = "Dokunmatik akış bağlı değil"
            _uiState.value = AudioUiState.IDLE
            controlJob?.cancel()
            sessionActive = false
            return
        }
        val open = try {
            withTimeout(OPEN_WAIT_MS) { opened.await() }
        } catch (_: Exception) {
            _error.value = "Masaüstü yanıt vermedi"
            _uiState.value = AudioUiState.IDLE
            controlJob?.cancel()
            sessionActive = false
            return
        }
        if (open == null) {
            _error.value = "Masaüstü sesi kapattı"
            _uiState.value = AudioUiState.IDLE
            controlJob?.cancel()
            sessionActive = false
            return
        }
        _codecLabel.value = if (open.opus) "OPUS" else "PCM"
        val codec = if (open.opus) AudioSessionCodec.OPUS else AudioSessionCodec.PCM
        val capture = if (_source.value == AudioSourceOption.SYSTEM) {
            AudioCaptureSource.SYSTEM
        } else {
            AudioCaptureSource.MIC
        }
        client.micMuted = _micMuted.value
        client.speakerMuted = _speakerMuted.value
        statsJob?.cancel()
        statsJob = ioScope.launch {
            client.stats.collect { stats -> updateMeta(stats) }
        }
        linkJob?.cancel()
        linkJob = ioScope.launch {
            client.state.collect { link ->
                _uiState.value = when (link) {
                    AudioLinkState.Streaming -> AudioUiState.ACTIVE
                    AudioLinkState.Connecting -> AudioUiState.OPENING
                    is AudioLinkState.Error -> {
                        _error.value = link.reason
                        AudioUiState.ERROR
                    }
                    AudioLinkState.Idle -> if (sessionActive) AudioUiState.OPENING else _uiState.value
                }
            }
        }
        client.start(device.host, open.port, device.fingerprint, codec, capture)
    }

    private fun teardown(final: AudioUiState) {
        sessionActive = false
        controlJob?.cancel()
        controlJob = null
        linkJob?.cancel()
        linkJob = null
        statsJob?.cancel()
        statsJob = null
        try {
            client.stop()
        } catch (_: Exception) {
        }
        _meta.value = ""
        _sent.value = 0L
        _received.value = 0L
        _uiState.value = final
    }

    private fun updateMeta(stats: AudioStats) {
        _sent.value = stats.sent
        _received.value = stats.received
        val drops = stats.droppedSend + stats.droppedReceive
        _meta.value = _codecLabel.value + " 48K STEREO · GÖN " + stats.sent +
            " · AL " + stats.received + " · ATILAN " + drops
    }

    private suspend fun activeDevice(): TrustedDevice? {
        return try {
            combine(
                AppGraph.deviceStore.devices,
                AppGraph.deviceStore.activeFingerprint,
            ) { devices, fingerprint ->
                devices.find { it.fingerprint == fingerprint }
            }.first()
        } catch (_: IOException) {
            null
        } catch (_: Exception) {
            null
        }
    }

    private fun openBody() = buildJsonObject {
        put("t", JsonPrimitive("open"))
        put("codec", JsonPrimitive("pcm"))
    }

    private fun releaseBody() = buildJsonObject {
        put("t", JsonPrimitive("release"))
    }

    private data class AudioOpen(val port: Int, val opus: Boolean)

    private companion object {
        const val KIND_AUDIO = "nb.audio"
        const val OPEN_WAIT_MS = 10000L
    }
}
