package bit.kedybaykedy.northbridge.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.Evdev
import bit.kedybaykedy.northbridge.transport.InputMsg
import bit.kedybaykedy.northbridge.transport.MouseButton
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class TouchpadViewModel : ViewModel() {

    private val _ctrlLatched = MutableStateFlow(false)
    val ctrlLatched: StateFlow<Boolean> = _ctrlLatched.asStateFlow()

    private val _altLatched = MutableStateFlow(false)
    val altLatched: StateFlow<Boolean> = _altLatched.asStateFlow()

    private val _keyboardOpen = MutableStateFlow(false)
    val keyboardOpen: StateFlow<Boolean> = _keyboardOpen.asStateFlow()

    private var latchGeneration = 0

    fun send(msg: InputMsg) {
        AppGraph.inputChannel.send(msg)
    }

    fun pressButton(button: MouseButton, down: Boolean) {
        send(InputMsg.Button(button, down))
    }

    fun tapKey(code: Int) {
        emitWithLatchedModifiers(code)
    }

    fun tapSpecial(code: Int) {
        emitWithLatchedModifiers(code)
    }

    fun typeChar(char: Char) {
        val code = Evdev.usPositionCode(char)
        if ((_ctrlLatched.value || _altLatched.value) && code != null) {
            emitWithLatchedModifiers(code)
        } else {
            clearLatches()
            send(InputMsg.Type(char.toString()))
        }
    }

    fun toggleCtrl() {
        _ctrlLatched.value = !_ctrlLatched.value
        scheduleLatchAutoClear()
    }

    fun toggleAlt() {
        _altLatched.value = !_altLatched.value
        scheduleLatchAutoClear()
    }

    fun setKeyboardOpen(open: Boolean) {
        _keyboardOpen.value = open
    }

    fun toggleKeyboard() {
        _keyboardOpen.value = !_keyboardOpen.value
    }

    private fun emitWithLatchedModifiers(code: Int) {
        val ctrl = _ctrlLatched.value
        val alt = _altLatched.value
        clearLatches()
        if (ctrl) send(InputMsg.Key(Evdev.KEY_LEFTCTRL, true))
        if (alt) send(InputMsg.Key(Evdev.KEY_LEFTALT, true))
        send(InputMsg.Key(code, true))
        send(InputMsg.Key(code, false))
        if (alt) send(InputMsg.Key(Evdev.KEY_LEFTALT, false))
        if (ctrl) send(InputMsg.Key(Evdev.KEY_LEFTCTRL, false))
    }

    private fun clearLatches() {
        latchGeneration += 1
        _ctrlLatched.value = false
        _altLatched.value = false
    }

    private fun scheduleLatchAutoClear() {
        if (!_ctrlLatched.value && !_altLatched.value) {
            latchGeneration += 1
            return
        }
        val generation = ++latchGeneration
        viewModelScope.launch {
            delay(LATCH_AUTO_CLEAR_MS)
            if (generation == latchGeneration) {
                _ctrlLatched.value = false
                _altLatched.value = false
            }
        }
    }

    companion object {
        private const val LATCH_AUTO_CLEAR_MS = 3000L
    }
}
