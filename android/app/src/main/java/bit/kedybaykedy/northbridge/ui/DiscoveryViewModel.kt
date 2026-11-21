package bit.kedybaykedy.northbridge.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.DiscoveredDevice
import bit.kedybaykedy.northbridge.data.TrustedDevice
import bit.kedybaykedy.northbridge.discovery.HeardAnnouncement
import bit.kedybaykedy.northbridge.discovery.MulticastDiscovery
import bit.kedybaykedy.northbridge.pairing.PairingClient
import bit.kedybaykedy.northbridge.transport.HttpApi
import java.util.concurrent.ConcurrentHashMap
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

    private val discovery = MulticastDiscovery(AppGraph.appContext)
    private val lastProbeAt = ConcurrentHashMap<String, Long>()

    private val _state = MutableStateFlow(DiscoveryUiState())
    val state: StateFlow<DiscoveryUiState> = _state.asStateFlow()

    fun startDiscovery() {
        discovery.start(viewModelScope) { heard -> onHeard(heard) }
    }

    override fun onCleared() {
        discovery.stop()
    }

    private fun onHeard(heard: HeardAnnouncement) {
        val message = heard.message
        val now = System.currentTimeMillis()
        val key = "${heard.host}:${message.port}"
        _state.update { state ->
            val existing = state.devices.find { it.key == key }
            val updated = if (existing != null) {
                existing.copy(
                    alias = message.alias,
                    version = message.version,
                    deviceType = message.deviceType,
                    lastSeen = now,
                )
            } else {
                DiscoveredDevice(
                    host = heard.host,
                    port = message.port,
                    alias = message.alias,
                    fingerprint = null,
                    version = message.version,
                    deviceType = message.deviceType,
                    lastSeen = now,
                )
            }
            state.copy(devices = state.devices.filterNot { it.key == key } + updated)
        }
        viewModelScope.launch {
            probe(key, heard.host, message.port, force = false)
        }
    }

    fun probeManual(address: String) {
        val parsed = parseAddress(address)
        if (parsed == null) {
            _state.update { it.copy(message = "Invalid address — use host, host:port or [v6]:port") }
            return
        }
        val (host, port) = parsed
        viewModelScope.launch {
            val probed = probe("$host:$port", host, port, force = true)
            _state.update {
                it.copy(message = if (probed) "Probed $host:$port" else "$host:$port unreachable")
            }
        }
    }

    private suspend fun probe(key: String, host: String, port: Int, force: Boolean): Boolean {
        val now = System.currentTimeMillis()
        if (!force && now - (lastProbeAt[key] ?: 0L) < PROBE_THROTTLE_MS) return false
        lastProbeAt[key] = now
        val result = HttpApi.probe(host, port, AppGraph.identity) ?: return false
        _state.update { state ->
            val entry = DiscoveredDevice(
                host = host,
                port = port,
                alias = result.alias,
                fingerprint = result.fingerprint,
                version = result.version,
                deviceType = result.deviceType,
                lastSeen = System.currentTimeMillis(),
            )
            state.copy(devices = state.devices.filterNot { it.key == key } + entry)
        }
        return true
    }

    fun requestPair(device: DiscoveredDevice) {
        if (device.fingerprint == null) {
            _state.update { it.copy(message = "Waiting for probe of ${device.host}:${device.port}") }
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
        private const val PROBE_THROTTLE_MS = 4000L

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
