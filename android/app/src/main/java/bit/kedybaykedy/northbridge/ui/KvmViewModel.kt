package bit.kedybaykedy.northbridge.ui

import android.content.res.Resources
import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.MirrorInjector
import bit.kedybaykedy.northbridge.input.NativePointer
import bit.kedybaykedy.northbridge.transport.ConnectionState
import bit.kedybaykedy.northbridge.transport.InputMsg
import bit.kedybaykedy.northbridge.transport.KIND_MIRROR
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

enum class KvmPhase {
    WAITING,
    ACTIVE,
    RELEASED
}

class KvmViewModel : ViewModel() {

    private val injector = MirrorInjector()

    private val _phase = MutableStateFlow(KvmPhase.WAITING)
    val phase: StateFlow<KvmPhase> = _phase.asStateFlow()

    private var captureHeld = false
    private var userReleased = false

    init {
        val dm = Resources.getSystem().displayMetrics
        injector.setScreenSize(dm.widthPixels, dm.heightPixels)
        viewModelScope.launch {
            AppGraph.inputChannel.messages.collect { msg ->
                when (msg) {
                    is InputMsg.Enter -> {
                        userReleased = false
                        _phase.value = KvmPhase.ACTIVE
                    }
                    is InputMsg.Move -> {
                        if (!userReleased) {
                            _phase.value = KvmPhase.ACTIVE
                        }
                    }
                    InputMsg.Leave -> {
                        injector.reset()
                        captureHeld = false
                        _phase.value = if (userReleased) KvmPhase.RELEASED else KvmPhase.WAITING
                    }
                    else -> Unit
                }
            }
        }
        viewModelScope.launch {
            AppGraph.inputChannel.kvmBodies.collect { body ->
                try {
                    injector.handlePacket(body)
                } catch (_: Exception) {
                }
            }
        }
        viewModelScope.launch {
            AppGraph.inputChannel.state.collect { state ->
                if (state != ConnectionState.CONNECTED && _phase.value == KvmPhase.ACTIVE) {
                    injector.reset()
                    captureHeld = false
                    _phase.value = KvmPhase.WAITING
                }
            }
        }
    }

    fun setScreenSize(width: Int, height: Int) {
        injector.setScreenSize(width, height)
    }

    fun requestCapture() {
        Log.d("northbridge", "kvm capture req held=" + captureHeld + " phase=" + _phase.value)
        if (captureHeld) return
        captureHeld = true
        userReleased = false
        if (_phase.value != KvmPhase.ACTIVE) {
            _phase.value = KvmPhase.WAITING
        }
        viewModelScope.launch(Dispatchers.IO) {
            NativePointer.sessionStart()
            val ok = AppGraph.inputChannel.sendPacket(KIND_MIRROR, grabBody())
            Log.d("northbridge", "kvm grab sent=" + ok)
        }
    }

    fun releaseCapture() {
        userReleased = true
        if (captureHeld) {
            captureHeld = false
            AppGraph.inputChannel.sendPacket(KIND_MIRROR, releaseBody())
        }
        injector.reset()
        _phase.value = KvmPhase.RELEASED
    }

    fun releaseOnLeave() {
        if (captureHeld) {
            captureHeld = false
            AppGraph.inputChannel.sendPacket(KIND_MIRROR, releaseBody())
        }
        injector.reset()
        userReleased = false
        _phase.value = KvmPhase.WAITING
    }

    override fun onCleared() {
        if (captureHeld) {
            captureHeld = false
            AppGraph.inputChannel.sendPacket(KIND_MIRROR, releaseBody())
        }
        injector.reset()
    }

    fun clickAt(x: Double, y: Double) {
        injector.tap(x.coerceIn(0.0, 1.0), y.coerceIn(0.0, 1.0))
    }

    private fun grabBody() = buildJsonObject {
        put("t", JsonPrimitive("kvm_grab"))
    }

    private fun releaseBody() = buildJsonObject {
        put("t", JsonPrimitive("kvm_release"))
    }
}
