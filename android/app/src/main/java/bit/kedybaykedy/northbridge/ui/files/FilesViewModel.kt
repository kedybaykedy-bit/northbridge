package bit.kedybaykedy.northbridge.ui.files

import android.net.Uri
import android.provider.OpenableColumns
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.TrustedDevice
import bit.kedybaykedy.northbridge.transport.FileClient
import bit.kedybaykedy.northbridge.transport.FileEvent
import bit.kedybaykedy.northbridge.transport.FileServer
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

data class ReceivedFile(
    val name: String,
    val size: Long,
    val location: String,
    val at: Long,
)

class FilesViewModel : ViewModel() {

    private val _deviceAlias = MutableStateFlow("")
    val deviceAlias: StateFlow<String> = _deviceAlias.asStateFlow()

    private val _sending = MutableStateFlow(false)
    val sending: StateFlow<Boolean> = _sending.asStateFlow()

    private val _status = MutableStateFlow("HAZIR")
    val status: StateFlow<String> = _status.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private val _received = MutableStateFlow<List<ReceivedFile>>(emptyList())
    val received: StateFlow<List<ReceivedFile>> = _received.asStateFlow()

    init {
        FileServer.ensureRunning()
        viewModelScope.launch {
            combine(
                AppGraph.deviceStore.devices,
                AppGraph.deviceStore.activeFingerprint,
            ) { devices, fingerprint ->
                devices.find { it.fingerprint == fingerprint }
            }.collect { device ->
                _deviceAlias.value = device?.alias ?: ""
            }
        }
        viewModelScope.launch {
            FileServer.events.collect { event ->
                when (event) {
                    is FileEvent.Received -> {
                        _received.value = listOf(
                            ReceivedFile(event.name, event.size, event.location, System.currentTimeMillis()),
                        ) + _received.value.take(19)
                        _status.value = "ALINDI: ${event.name}"
                        _error.value = null
                    }
                    is FileEvent.Failed -> {
                        _status.value = "ALIM BAŞARISIZ"
                        _error.value = event.reason
                    }
                }
            }
        }
    }

    fun clearError() {
        _error.value = null
    }

    fun send(uri: Uri) {
        if (_sending.value) return
        viewModelScope.launch(Dispatchers.IO) {
            _sending.value = true
            _error.value = null
            try {
                val device = activeDevice()
                    ?: throw IOException("önce Cihazlar sekmesinde bir masaüstü seç")
                val meta = readMeta(uri) ?: throw IOException("dosya okunamadı")
                _status.value = "GÖNDERİLİYOR: ${meta.name}"
                val resolver = AppGraph.appContext.contentResolver
                val mime = resolver.getType(uri) ?: "application/octet-stream"
                FileClient.upload(
                    device = device,
                    identity = AppGraph.identity,
                    fileName = meta.name,
                    mime = mime,
                    size = meta.size,
                    open = { resolver.openInputStream(uri) ?: throw IOException("dosya açılamadı") },
                    onProgress = { sent, total ->
                        val pct = if (total <= 0) 100 else (sent * 100 / total).toInt()
                        _status.value = "GÖNDERİLİYOR: ${meta.name} %$pct"
                    },
                )
                _status.value = "GÖNDERİLDİ: ${meta.name} → ${device.alias}"
            } catch (e: IOException) {
                _status.value = "GÖNDERİM BAŞARISIZ"
                _error.value = e.message ?: "bilinmeyen hata"
            } catch (e: Exception) {
                _status.value = "GÖNDERİM BAŞARISIZ"
                _error.value = e.message ?: "bilinmeyen hata"
            } finally {
                _sending.value = false
            }
        }
    }

    private suspend fun activeDevice(): TrustedDevice? {
        return try {
            combine(
                AppGraph.deviceStore.devices,
                AppGraph.deviceStore.activeFingerprint,
            ) { devices, fingerprint ->
                devices.find { it.fingerprint == fingerprint }
            }.first()
        } catch (_: Exception) {
            null
        }
    }

    private data class PickedMeta(val name: String, val size: Long)

    private fun readMeta(uri: Uri): PickedMeta? {
        val resolver = AppGraph.appContext.contentResolver
        resolver.query(uri, null, null, null, null)?.use { cursor ->
            val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
            if (cursor.moveToFirst()) {
                val name = if (nameIndex >= 0) cursor.getString(nameIndex) else null
                val size = if (sizeIndex >= 0) cursor.getLong(sizeIndex) else -1L
                if (!name.isNullOrBlank() && size >= 0) {
                    return PickedMeta(name, size)
                }
            }
        }
        return null
    }
}
