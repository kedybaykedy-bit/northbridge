package bit.kedybaykedy.northbridge.discovery

import android.util.Log
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.DiscoveredDevice
import bit.kedybaykedy.northbridge.transport.HttpApi
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class DiscoveryHub {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val discovery = MulticastDiscovery(AppGraph.appContext)
    private val lastProbeAt = ConcurrentHashMap<String, Long>()

    private val _devices = MutableStateFlow<List<DiscoveredDevice>>(emptyList())
    val devices: StateFlow<List<DiscoveredDevice>> = _devices.asStateFlow()

    private val _running = MutableStateFlow(false)
    val running: StateFlow<Boolean> = _running.asStateFlow()

    fun start() {
        if (_running.value) return
        _running.value = true
        discovery.start(scope) { heard -> onHeard(heard) }
    }

    fun stop() {
        _running.value = false
        discovery.stop()
    }

    private fun onHeard(heard: HeardAnnouncement) {
        val message = heard.message
        val now = System.currentTimeMillis()
        val key = "${heard.host}:${message.port}"
        _devices.update { list ->
            val existing = list.find { it.key == key }
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
            list.filterNot { it.key == key } + updated
        }
        scope.launch {
            probe(key, heard.host, message.port, force = false)
        }
    }

    suspend fun probe(key: String, host: String, port: Int, force: Boolean): Boolean {
        val now = System.currentTimeMillis()
        if (!force && now - (lastProbeAt[key] ?: 0L) < PROBE_THROTTLE_MS) return false
        lastProbeAt[key] = now
        val result = HttpApi.probe(host, port, AppGraph.identity) ?: return false
        val fingerprint = result.fingerprint
        if (fingerprint.isNotEmpty()) {
            AppGraph.deviceStore.observeAddress(fingerprint, host, port)
        }
        _devices.update { list ->
            val entry = DiscoveredDevice(
                host = host,
                port = port,
                alias = result.alias,
                fingerprint = fingerprint,
                version = result.version,
                deviceType = result.deviceType,
                lastSeen = System.currentTimeMillis(),
            )
            list.filterNot { it.key == key } + entry
        }
        return true
    }

    companion object {
        private const val PROBE_THROTTLE_MS = 4000L
    }
}
