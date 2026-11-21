package bit.kedybaykedy.northbridge.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.kvm.CursorPos
import bit.kedybaykedy.northbridge.transport.InputMsg
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

enum class KvmPhase {
    WAITING,
    ACTIVE,
    RELEASED
}

class KvmViewModel : ViewModel() {

    private val _cursor = MutableStateFlow<CursorPos?>(null)
    val cursor: StateFlow<CursorPos?> = _cursor.asStateFlow()

    private val _phase = MutableStateFlow(KvmPhase.WAITING)
    val phase: StateFlow<KvmPhase> = _phase.asStateFlow()

    private var captureHeld = false
    private var userReleased = false

    init {
        viewModelScope.launch {
            AppGraph.inputChannel.messages.collect { msg ->
                when (msg) {
                    is InputMsg.Enter -> {
                        userReleased = false
                        _cursor.value = CursorPos(msg.x.toFloat(), msg.y.toFloat())
                        _phase.value = KvmPhase.ACTIVE
                    }
                    is InputMsg.Move -> {
                        _cursor.value = CursorPos(msg.x.toFloat(), msg.y.toFloat())
                        if (!userReleased) {
                            _phase.value = KvmPhase.ACTIVE
                        }
                    }
                    InputMsg.Leave -> {
                        _cursor.value = null
                        captureHeld = false
                        _phase.value = if (userReleased) KvmPhase.RELEASED else KvmPhase.WAITING
                    }
                    else -> Unit
                }
            }
        }
    }

    fun requestCapture() {
        if (captureHeld) return
        captureHeld = true
        userReleased = false
        if (_phase.value != KvmPhase.ACTIVE) {
            _phase.value = KvmPhase.WAITING
        }
        AppGraph.inputChannel.send(InputMsg.Capture)
    }

    fun releaseCapture() {
        userReleased = true
        if (captureHeld) {
            captureHeld = false
            AppGraph.inputChannel.send(InputMsg.Release)
        }
        _phase.value = KvmPhase.RELEASED
    }

    fun releaseOnLeave() {
        if (captureHeld) {
            captureHeld = false
            AppGraph.inputChannel.send(InputMsg.Release)
        }
        userReleased = false
        _cursor.value = null
        _phase.value = KvmPhase.WAITING
    }

    override fun onCleared() {
        if (captureHeld) {
            captureHeld = false
            AppGraph.inputChannel.send(InputMsg.Release)
        }
    }

    fun clickAt(x: Double, y: Double) {
        AppGraph.inputChannel.send(
            InputMsg.ClickAbs(x.coerceIn(0.0, 1.0), y.coerceIn(0.0, 1.0)),
        )
    }
}
