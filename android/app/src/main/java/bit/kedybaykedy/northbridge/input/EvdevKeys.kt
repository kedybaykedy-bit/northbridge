package bit.kedybaykedy.northbridge.input

import bit.kedybaykedy.northbridge.transport.InputMsg

object Evdev {

    const val KEY_ESC = 1
    const val KEY_BACKSPACE = 14
    const val KEY_TAB = 15
    const val KEY_ENTER = 28
    const val KEY_LEFTCTRL = 29
    const val KEY_LEFTSHIFT = 42
    const val KEY_LEFTALT = 56
    const val KEY_SPACE = 57
    const val KEY_HOME = 102
    const val KEY_UP = 103
    const val KEY_PAGEUP = 104
    const val KEY_LEFT = 105
    const val KEY_RIGHT = 106
    const val KEY_END = 107
    const val KEY_DOWN = 108
    const val KEY_PAGEDOWN = 109
    const val KEY_F1 = 59
    const val KEY_F2 = 60
    const val KEY_F3 = 61
    const val KEY_F4 = 62
    const val KEY_F5 = 63
    const val KEY_F6 = 64
    const val KEY_F7 = 65
    const val KEY_F8 = 66
    const val KEY_F9 = 67
    const val KEY_F10 = 68
    const val KEY_F11 = 87
    const val KEY_F12 = 88

    private val unshifted = mapOf(
        'a' to 30,
        'b' to 48,
        'c' to 46,
        'd' to 32,
        'e' to 18,
        'f' to 33,
        'g' to 34,
        'h' to 35,
        'i' to 23,
        'j' to 36,
        'k' to 37,
        'l' to 38,
        'm' to 50,
        'n' to 49,
        'o' to 24,
        'p' to 25,
        'q' to 16,
        'r' to 19,
        's' to 31,
        't' to 20,
        'u' to 22,
        'v' to 47,
        'w' to 17,
        'x' to 45,
        'y' to 21,
        'z' to 44,
        '1' to 2,
        '2' to 3,
        '3' to 4,
        '4' to 5,
        '5' to 6,
        '6' to 7,
        '7' to 8,
        '8' to 9,
        '9' to 10,
        '0' to 11,
        ' ' to 57,
        '-' to 12,
        '=' to 13,
        '[' to 26,
        ']' to 27,
        '\\' to 43,
        ';' to 39,
        '\'' to 40,
        '`' to 41,
        ',' to 51,
        '.' to 52,
        '/' to 53,
    )

    private val shifted = mapOf(
        '!' to 2,
        '@' to 3,
        '#' to 4,
        '$' to 5,
        '%' to 6,
        '^' to 7,
        '&' to 8,
        '*' to 9,
        '(' to 10,
        ')' to 11,
        '_' to 12,
        '+' to 13,
        '{' to 26,
        '}' to 27,
        '|' to 43,
        ':' to 39,
        '"' to 40,
        '~' to 41,
        '<' to 51,
        '>' to 52,
        '?' to 53,
    )

    fun keyEventsFor(text: String): List<InputMsg.Key> {
        val events = mutableListOf<InputMsg.Key>()
        for (char in text) {
            when {
                char == '\n' -> {
                    events += InputMsg.Key(KEY_ENTER, true)
                    events += InputMsg.Key(KEY_ENTER, false)
                }

                char == '\t' -> {
                    events += InputMsg.Key(KEY_TAB, true)
                    events += InputMsg.Key(KEY_TAB, false)
                }

                else -> {
                    val code: Int
                    val shift: Boolean
                    when {
                        char in unshifted -> {
                            code = unshifted.getValue(char)
                            shift = false
                        }

                        char in shifted -> {
                            code = shifted.getValue(char)
                            shift = true
                        }

                        char.isUpperCase() && char.lowercaseChar() in unshifted -> {
                            code = unshifted.getValue(char.lowercaseChar())
                            shift = true
                        }

                        else -> continue
                    }
                    if (shift) events += InputMsg.Key(KEY_LEFTSHIFT, true)
                    events += InputMsg.Key(code, true)
                    events += InputMsg.Key(code, false)
                    if (shift) events += InputMsg.Key(KEY_LEFTSHIFT, false)
                }
            }
        }
        return events
    }

    private val usPosition = mapOf(
        'a' to 30,
        'b' to 48,
        'c' to 46,
        'd' to 32,
        'e' to 18,
        'f' to 33,
        'g' to 34,
        'h' to 35,
        'i' to 23,
        'j' to 36,
        'k' to 37,
        'l' to 38,
        'm' to 50,
        'n' to 49,
        'o' to 24,
        'p' to 25,
        'q' to 16,
        'r' to 19,
        's' to 31,
        't' to 20,
        'u' to 22,
        'v' to 47,
        'w' to 17,
        'x' to 45,
        'y' to 21,
        'z' to 44,
        '1' to 2,
        '2' to 3,
        '3' to 4,
        '4' to 5,
        '5' to 6,
        '6' to 7,
        '7' to 8,
        '8' to 9,
        '9' to 10,
        '0' to 11,
        ' ' to 57,
    )

    fun usPositionCode(char: Char): Int? {
        if (char == ' ') return KEY_SPACE
        val lower = char.lowercaseChar()
        return usPosition[lower]
    }
}
