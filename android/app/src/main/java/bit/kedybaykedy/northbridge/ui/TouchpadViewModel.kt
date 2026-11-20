package bit.kedybaykedy.northbridge.ui

import androidx.lifecycle.ViewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.Evdev
import bit.kedybaykedy.northbridge.transport.InputMsg
import bit.kedybaykedy.northbridge.transport.MouseButton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

class TouchpadViewModel : ViewModel() {

    private val _ctrlLatched = MutableStateFlow(false)
    val ctrlLatched: StateFlow<Boolean> = _ctrlLatched.asStateFlow()

    fun send(msg: InputMsg) {
        AppGraph.inputChannel.send(msg)
    }

    fun pressButton(button: MouseButton, down: Boolean) {
        send(InputMsg.Button(button, down))
    }

    fun tapKey(code: Int) {
        send(InputMsg.Key(code, true))
        send(InputMsg.Key(code, false))
    }

    fun toggleCtrl() {
        val down = !_ctrlLatched.value
        _ctrlLatched.value = down
        send(InputMsg.Key(Evdev.KEY_LEFTCTRL, down))
    }

    fun submitText(text: String) {
        for (event in Evdev.keyEventsFor(text)) {
            send(event)
        }
    }

    override fun onCleared() {
        if (_ctrlLatched.value) {
            send(InputMsg.Key(Evdev.KEY_LEFTCTRL, false))
        }
    }
}
