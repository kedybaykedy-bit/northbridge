package bit.kedybaykedy.northbridge.ui.mirror

import android.content.Intent
import android.content.res.Resources
import androidx.lifecycle.ViewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.TrustedDevice
import bit.kedybaykedy.northbridge.input.MirrorInjector
import bit.kedybaykedy.northbridge.keepalive.MirrorKeepAliveService
import bit.kedybaykedy.northbridge.transport.MirrorClient
import bit.kedybaykedy.northbridge.transport.MirrorLinkState
import java.io.IOException
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

enum class MirrorUiState {
    IDLE,
    MIRRORING,
    ERROR,
}

class MirrorViewModel : ViewModel() {

    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client = MirrorClient(AppGraph.identity, ioScope, AppGraph.inputChannel)
    private val injector = MirrorInjector()

    private val _uiState = MutableStateFlow(MirrorUiState.IDLE)
    val uiState: StateFlow<MirrorUiState> = _uiState.asStateFlow()

    private val _meta = MutableStateFlow("")
    val meta: StateFlow<String> = _meta.asStateFlow()

    private val _deviceAlias = MutableStateFlow("")
    val deviceAlias: StateFlow<String> = _deviceAlias.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private var inputsJob: Job? = null
    private var stateJob: Job? = null
    private var statsJob: Job? = null

    init {
        val dm = Resources.getSystem().displayMetrics
        injector.setScreenSize(dm.widthPixels, dm.heightPixels)
        stateJob = ioScope.launch {
            client.state.collect { link ->
                when (link) {
                    MirrorLinkState.Idle -> {
                        _uiState.value = MirrorUiState.IDLE
                        _meta.value = ""
                    }
                    MirrorLinkState.Opening -> {
                        _uiState.value = MirrorUiState.IDLE
                        _meta.value = "MASAUSTU YANITI BEKLENIYOR..."
                    }
                    MirrorLinkState.Live -> {
                        _uiState.value = MirrorUiState.MIRRORING
                        _error.value = null
                    }
                    is MirrorLinkState.Error -> {
                        _uiState.value = MirrorUiState.ERROR
                        _error.value = link.reason
                        _meta.value = ""
                    }
                }
            }
        }
        statsJob = ioScope.launch {
            client.stats.collect { stats ->
                _meta.value = if (stats == null) {
                    ""
                } else {
                    "" + stats.kbps + " KBPS \u00B7 " + stats.fps + " FPS \u00B7 " +
                        stats.width + "X" + stats.height + " \u00B7 SEQ " + stats.seq
                }
            }
        }
        inputsJob = ioScope.launch {
            client.inputs.collect { body ->
                try {
                    injector.handlePacket(body)
                } catch (_: Exception) {
                }
            }
        }
    }

    fun start() {
        ioScope.launch {
            _error.value = null
            val device = activeDevice()
            if (device == null) {
                _error.value = "ONCE CIHAZLAR SEKMESINDE BIR MASAUSTU ESLESTIR"
                _uiState.value = MirrorUiState.ERROR
                return@launch
            }
            _deviceAlias.value = device.alias
            client.startMirror(device.host, device.packetPort, device.fingerprint, FPS, MAX_W)
            try {
                AppGraph.appContext.startForegroundService(
                    Intent(AppGraph.appContext, MirrorKeepAliveService::class.java),
                )
            } catch (_: Exception) {
            }
        }
    }

    fun stop() {
        client.stopMirror()
        stopKeepAlive()
    }

    private fun stopKeepAlive() {
        try {
            AppGraph.appContext.stopService(
                Intent(AppGraph.appContext, MirrorKeepAliveService::class.java),
            )
        } catch (_: Exception) {
        }
    }

    override fun onCleared() {
        inputsJob?.cancel()
        stateJob?.cancel()
        statsJob?.cancel()
        try {
            client.stopMirror()
        } catch (_: Exception) {
        }
        stopKeepAlive()
        ioScope.cancel()
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

    private companion object {
        const val FPS = 30
        const val MAX_W = 1080
    }
}
