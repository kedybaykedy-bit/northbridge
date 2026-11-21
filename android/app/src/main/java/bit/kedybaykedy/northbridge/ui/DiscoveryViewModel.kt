package bit.kedybaykedy.northbridge.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.DiscoveredDevice
import bit.kedybaykedy.northbridge.data.TrustedDevice
import bit.kedybaykedy.northbridge.pairing.PairingClient
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlin.random.Random

data class PairDialogState(
    val device: DiscoveredDevice,
    val pin: String,
)

data class DiscoveryUiState(
    val devices: List<DiscoveredDevice> = emptyList(),
    val pairing: Boolean = false,
    val pairDialog: PairDialogState? = null,
    val message: String? = null,
)

class DiscoveryViewModel : ViewModel() {

    private val hub = AppGraph.discoveryHub

    private val _state = MutableStateFlow(DiscoveryUiState())
    val state: StateFlow<DiscoveryUiState> = _state.asStateFlow()

    private val _paired = MutableStateFlow<Set<String>>(emptySet())
    val paired: StateFlow<Set<String>> = _paired.asStateFlow()

    init {
        viewModelScope.launch {
            hub.devices.collect { devices ->
                _state.update { it.copy(devices = devices) }
            }
        }
        viewModelScope.launch {
            AppGraph.deviceStore.devices.collect { trusted ->
                _paired.value = trusted.map { it.fingerprint }.toSet()
            }
        }
    }

    fun startDiscovery() {
        hub.start()
    }

    fun probeManual(address: String) {
        val parsed = parseAddress(address)
        if (parsed == null) {
            _state.update { it.copy(message = "Invalid address — use host, host:port or [v6]:port") }
            return
        }
        val (host, port) = parsed
        viewModelScope.launch {
            val probed = hub.probe("$host:$port", host, port, force = true)
            _state.update {
                it.copy(message = if (probed) "Probed $host:$port" else "$host:$port unreachable")
            }
        }
    }

    fun requestPair(device: DiscoveredDevice) {
        if (device.fingerprint == null) {
            _state.update { it.copy(message = "Waiting for probe of ${device.host}:${device.port}") }
            return
        }
        if (device.fingerprint in _paired.value) {
            _state.update { it.copy(message = "Already paired with ${device.alias}") }
            return
        }
        val pin = Random.nextInt(10000).toString().padStart(4, '0')
        _state.update { it.copy(pairDialog = PairDialogState(device, pin)) }
    }

    fun cancelPair() {
        _state.update { it.copy(pairDialog = null) }
    }

    fun confirmPair() {
        val dialog = _state.value.pairDialog ?: return
        val fingerprint = dialog.device.fingerprint ?: return
        _state.update { it.copy(pairDialog = null, pairing = true, message = null) }
        viewModelScope.launch {
            val outcome = try {
                PairingClient.pair(
                    host = dialog.device.host,
                    httpPort = dialog.device.port,
                    fingerprint = fingerprint,
                    identity = AppGraph.identity,
                    pin = dialog.pin,
                )
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (_: Exception) {
                null
            }
            if (outcome != null && outcome.accepted) {
                AppGraph.deviceStore.upsert(
                    TrustedDevice(
                        fingerprint = fingerprint,
                        alias = outcome.alias.ifBlank { dialog.device.alias },
                        host = dialog.device.host,
                        httpPort = dialog.device.port,
                        pairedAt = System.currentTimeMillis(),
                        incoming = outcome.incoming,
                        outgoing = outcome.outgoing,
                    ),
                )
                _state.update {
                    it.copy(
                        pairing = false,
                        message = "Paired with ${outcome.alias.ifBlank { dialog.device.alias }}",
                    )
                }
            } else {
                _state.update {
                    it.copy(pairing = false, message = "Pairing with ${dialog.device.alias} failed")
                }
            }
        }
    }

    fun clearMessage() {
        _state.update { it.copy(message = null) }
    }

    companion object {
        fun parseAddress(raw: String): Pair<String, Int>? {
            val address = raw.trim()
            if (address.isEmpty()) return null
            if (address.startsWith("[")) {
                val end = address.indexOf(']')
                if (end <= 1) return null
                val host = address.substring(1, end)
                val rest = address.substring(end + 1)
                val port = if (rest.startsWith(":")) {
                    rest.substring(1).toIntOrNull()
                } else {
                    DEFAULT_PORT
                }
                return if (port != null && port in 1..65535) host to port else null
            }
            val colons = address.count { it == ':' }
            return when {
                colons == 0 -> address to DEFAULT_PORT
                colons == 1 -> {
                    val index = address.indexOf(':')
                    val port = address.substring(index + 1).toIntOrNull()
                    if (port != null && port in 1..65535) {
                        address.substring(0, index) to port
                    } else {
                        null
                    }
                }

                else -> address to DEFAULT_PORT
            }
        }

        private const val DEFAULT_PORT = 53317
    }
}
