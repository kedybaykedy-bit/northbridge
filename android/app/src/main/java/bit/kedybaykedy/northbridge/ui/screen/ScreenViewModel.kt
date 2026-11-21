package bit.kedybaykedy.northbridge.ui.screen

import androidx.lifecycle.ViewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.TrustedDevice
import bit.kedybaykedy.northbridge.transport.KIND_SCREEN
import bit.kedybaykedy.northbridge.transport.ScreenClient
import bit.kedybaykedy.northbridge.transport.ScreenFrame
import bit.kedybaykedy.northbridge.transport.ScreenLinkState
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

enum class ScreenUiState {
    IDLE,
    WAITING,
    LIVE,
}

class ScreenViewModel : ViewModel() {

    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client = ScreenClient(AppGraph.identity, ioScope)

    private val _uiState = MutableStateFlow(ScreenUiState.IDLE)
    val uiState: StateFlow<ScreenUiState> = _uiState.asStateFlow()

    private val _frame = MutableStateFlow<ScreenFrame?>(null)
    val frame: StateFlow<ScreenFrame?> = _frame.asStateFlow()

    private val _meta = MutableStateFlow("")
    val meta: StateFlow<String> = _meta.asStateFlow()

    private val _deviceAlias = MutableStateFlow("")
    val deviceAlias: StateFlow<String> = _deviceAlias.asStateFlow()

    private val _mono = MutableStateFlow(true)
    val mono: StateFlow<Boolean> = _mono.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    @Volatile
    private var sessionActive = false

    private var controlJob: Job? = null
    private var framesJob: Job? = null
    private var linkJob: Job? = null

    private var fpsWindowStart = 0L
    private var fpsCount = 0
    private var fps = 0

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
                AppGraph.inputChannel.sendPacket(KIND_SCREEN, releaseBody())
            } catch (_: Exception) {
            }
        }
        teardown()
    }

    fun setMono(value: Boolean) {
        _mono.value = value
    }

    override fun onCleared() {
        teardown()
        ioScope.cancel()
    }

    private suspend fun openSession() {
        _error.value = null
        _frame.value = null
        _meta.value = ""
        fpsWindowStart = 0L
        fpsCount = 0
        fps = 0
        val device = activeDevice()
        if (device == null) {
            _error.value = "Önce Cihazlar sekmesinde bir masaüstü eşleştir"
            _uiState.value = ScreenUiState.IDLE
            sessionActive = false
            return
        }
        _deviceAlias.value = device.alias
        _uiState.value = ScreenUiState.WAITING
        val opened = CompletableDeferred<Int?>()
        controlJob?.cancel()
        controlJob = ioScope.launch {
            AppGraph.inputChannel.controlPackets.collect { packet ->
                if (packet.kind != KIND_SCREEN) {
                    return@collect
                }
                val body = packet.body as? JsonObject ?: return@collect
                when ((body["t"] as? JsonPrimitive)?.content) {
                    "opened" -> {
                        if (!opened.isCompleted) {
                            val port = (body["port"] as? JsonPrimitive)?.content?.toIntOrNull()
                            opened.complete(port ?: (device.packetPort + 1))
                        }
                    }
                    "closed" -> {
                        if (!opened.isCompleted) {
                            opened.complete(null)
                        } else {
                            teardown()
                        }
                    }
                }
            }
        }
        val sent = AppGraph.inputChannel.sendPacket(KIND_SCREEN, openBody())
        if (!sent) {
            _error.value = "Dokunmatik akış bağlı değil"
            _uiState.value = ScreenUiState.IDLE
            controlJob?.cancel()
            sessionActive = false
            return
        }
        val port = try {
            withTimeout(OPEN_WAIT_MS) { opened.await() }
        } catch (_: Exception) {
            _error.value = "Masaüstü yanıt vermedi"
            _uiState.value = ScreenUiState.IDLE
            controlJob?.cancel()
            sessionActive = false
            return
        }
        if (port == null) {
            _error.value = "Masaüstü akışı kapattı"
            _uiState.value = ScreenUiState.IDLE
            controlJob?.cancel()
            sessionActive = false
            return
        }
        framesJob?.cancel()
        framesJob = ioScope.launch {
            client.frames.collect { screenFrame ->
                _frame.value = screenFrame
                updateMeta(screenFrame)
            }
        }
        linkJob?.cancel()
        linkJob = ioScope.launch {
            client.state.collect { link ->
                _uiState.value = when (link) {
                    ScreenLinkState.Streaming -> ScreenUiState.LIVE
                    ScreenLinkState.Connecting -> ScreenUiState.WAITING
                    is ScreenLinkState.Error -> ScreenUiState.WAITING
                    ScreenLinkState.Idle -> if (sessionActive) ScreenUiState.WAITING else ScreenUiState.IDLE
                }
            }
        }
        client.start(device.host, port, device.fingerprint)
    }

    private fun teardown() {
        sessionActive = false
        controlJob?.cancel()
        controlJob = null
        framesJob?.cancel()
        framesJob = null
        linkJob?.cancel()
        linkJob = null
        try {
            client.stop()
        } catch (_: Exception) {
        }
        _frame.value = null
        _meta.value = ""
        _uiState.value = ScreenUiState.IDLE
    }

    private fun updateMeta(screenFrame: ScreenFrame) {
        val now = System.currentTimeMillis()
        if (fpsWindowStart == 0L) {
            fpsWindowStart = now
            fpsCount = 0
        }
        fpsCount++
        val elapsed = now - fpsWindowStart
        if (elapsed >= 1000L) {
            fps = ((fpsCount * 1000L) / elapsed).toInt()
            fpsWindowStart = now
            fpsCount = 0
        }
        val drops = try {
            client.droppedFrames
        } catch (_: Exception) {
            0L
        }
        _meta.value = "SEQ " + screenFrame.seq + " · " + screenFrame.width + "x" +
            screenFrame.height + " · " + fps + " FPS · ATLANAN " + drops
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
        put("max_w", JsonPrimitive(1280))
        put("fps", JsonPrimitive(12))
    }

    private fun releaseBody() = buildJsonObject {
        put("t", JsonPrimitive("release"))
    }

    private companion object {
        const val OPEN_WAIT_MS = 10000L
    }
}
