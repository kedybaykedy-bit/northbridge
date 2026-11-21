package bit.kedybaykedy.northbridge.data

import android.content.SharedPreferences
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

class DeviceStore(private val prefs: SharedPreferences) {

    private val json = Json { ignoreUnknownKeys = true }

    private val _devices = MutableStateFlow(load())
    val devices: StateFlow<List<TrustedDevice>> = _devices.asStateFlow()

    private val _activeFingerprint = MutableStateFlow(prefs.getString(KEY_ACTIVE, null))
    val activeFingerprint: StateFlow<String?> = _activeFingerprint.asStateFlow()

    private fun load(): List<TrustedDevice> {
        val raw = prefs.getString(KEY_DEVICES, null) ?: return emptyList()
        return try {
            json.decodeFromString(raw)
        } catch (_: Exception) {
            emptyList()
        }
    }

    private fun persist(list: List<TrustedDevice>) {
        prefs.edit().putString(KEY_DEVICES, json.encodeToString(list)).apply()
        _devices.value = list
    }

    fun upsert(device: TrustedDevice) {
        val list = _devices.value.filterNot { it.fingerprint == device.fingerprint } + device
        persist(list)
        if (_activeFingerprint.value == null) {
            setActive(device.fingerprint)
        }
    }

    fun remove(fingerprint: String) {
        persist(_devices.value.filterNot { it.fingerprint == fingerprint })
        if (_activeFingerprint.value == fingerprint) {
            setActive(null)
        }
    }

    fun observeAddress(fingerprint: String, host: String, httpPort: Int) {
        val list = _devices.value
        val index = list.indexOfFirst { it.fingerprint == fingerprint }
        if (index < 0) return
        val existing = list[index]
        if (existing.host == host && existing.httpPort == httpPort) return
        val updated = list.toMutableList()
        updated[index] = existing.copy(host = host, httpPort = httpPort)
        persist(updated)
        Log.d(
            "northbridge",
            "address refreshed ${existing.alias}: ${existing.host}:${existing.httpPort} -> $host:$httpPort",
        )
    }

    fun setActive(fingerprint: String?) {
        _activeFingerprint.value = fingerprint
        prefs.edit().putString(KEY_ACTIVE, fingerprint).apply()
    }

    companion object {
        private const val KEY_DEVICES = "paired_devices"
        private const val KEY_ACTIVE = "active_fingerprint"
    }
}
