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

    private val _shiftLatched = MutableStateFlow(false)
    val shiftLatched: StateFlow<Boolean> = _shiftLatched.asStateFlow()

    private val _symLatched = MutableStateFlow(false)
    val symLatched: StateFlow<Boolean> = _symLatched.asStateFlow()

    private var latchGeneration = 0

    fun send(msg: InputMsg) {
        AppGraph.inputChannel.send(msg)
    }

    fun pressButton(button: MouseButton, down: Boolean) {
        send(InputMsg.Button(button, down))
    }

    fun clickButton(button: MouseButton) {
        send(InputMsg.Button(button, true))
        send(InputMsg.Button(button, false))
    }

    fun tapKey(code: Int) {
        emitWithLatchedModifiers(code, withShift = false)
    }

    fun tapSpecial(code: Int) {
        emitWithLatchedModifiers(code, withShift = false)
    }

    fun typeChar(char: Char) {
        val ctrl = _ctrlLatched.value
        val alt = _altLatched.value
        val shift = _shiftLatched.value
        val code = Evdev.usPositionCode(char)
        if ((ctrl || alt) && code != null) {
            emitWithLatchedModifiers(code, withShift = shift)
        } else {
            clearLatches()
            val out = if (shift) shiftedChar(char) else char
            send(InputMsg.Type(out.toString()))
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

    fun toggleShift() {
        _shiftLatched.value = !_shiftLatched.value
        scheduleLatchAutoClear()
    }

    fun toggleSym() {
        _symLatched.value = !_symLatched.value
        scheduleLatchAutoClear()
    }

    private fun emitWithLatchedModifiers(code: Int, withShift: Boolean) {
        val ctrl = _ctrlLatched.value
        val alt = _altLatched.value
        clearLatches()
        if (ctrl) send(InputMsg.Key(Evdev.KEY_LEFTCTRL, true))
        if (alt) send(InputMsg.Key(Evdev.KEY_LEFTALT, true))
        if (withShift) send(InputMsg.Key(Evdev.KEY_LEFTSHIFT, true))
        send(InputMsg.Key(code, true))
        send(InputMsg.Key(code, false))
        if (withShift) send(InputMsg.Key(Evdev.KEY_LEFTSHIFT, false))
        if (alt) send(InputMsg.Key(Evdev.KEY_LEFTALT, false))
        if (ctrl) send(InputMsg.Key(Evdev.KEY_LEFTCTRL, false))
    }

    private fun clearLatches() {
        latchGeneration += 1
        _ctrlLatched.value = false
        _altLatched.value = false
        _shiftLatched.value = false
        _symLatched.value = false
    }

    private fun scheduleLatchAutoClear() {
        if (!_ctrlLatched.value && !_altLatched.value && !_shiftLatched.value && !_symLatched.value) {
            latchGeneration += 1
            return
        }
        val generation = ++latchGeneration
        viewModelScope.launch {
            delay(LATCH_AUTO_CLEAR_MS)
            if (generation == latchGeneration) {
                _ctrlLatched.value = false
                _altLatched.value = false
                _shiftLatched.value = false
                _symLatched.value = false
            }
        }
    }

    companion object {
        private const val LATCH_AUTO_CLEAR_MS = 3000L

        private val digitSymbols = mapOf(
            '1' to '!',
            '2' to '@',
            '3' to '#',
            '4' to '$',
            '5' to '%',
            '6' to '^',
            '7' to '&',
            '8' to '*',
            '9' to '(',
            '0' to ')',
        )

        fun shiftedChar(char: Char): Char {
            digitSymbols[char]?.let { return it }
            return char.uppercaseChar()
        }
    }
}
