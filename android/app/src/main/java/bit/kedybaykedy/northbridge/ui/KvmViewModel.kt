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

class KvmViewModel : ViewModel() {

    private val _cursor = MutableStateFlow<CursorPos?>(null)
    val cursor: StateFlow<CursorPos?> = _cursor.asStateFlow()

    init {
        viewModelScope.launch {
            AppGraph.inputChannel.messages.collect { msg ->
                when (msg) {
                    is InputMsg.Enter -> _cursor.value = CursorPos(msg.x.toFloat(), msg.y.toFloat())
                    is InputMsg.Move -> _cursor.value = CursorPos(msg.x.toFloat(), msg.y.toFloat())
                    InputMsg.Leave -> _cursor.value = null
                    else -> Unit
                }
            }
        }
    }

    fun clickAt(x: Double, y: Double) {
        AppGraph.inputChannel.send(InputMsg.ClickAbs(x, y))
    }
}
