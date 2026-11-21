package bit.kedybaykedy.northbridge.input

import android.view.KeyEvent
import java.io.IOException
import java.io.OutputStream
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull

class MirrorInjector {

    private val lock = Any()

    private var proc: Process? = null
    private var stdin: OutputStream? = null

    @Volatile
    var errorCount = 0L
        private set

    @Volatile
    private var screenW = 1080

    @Volatile
    private var screenH = 1920

    @Volatile
    private var lastX = 0.5

    @Volatile
    private var lastY = 0.5

    @Volatile
    private var lastScrollAt = 0L

    private var dragArmed = false
    private var dragMoved = false
    private var dragStartX = 0
    private var dragStartY = 0
    private var dragLastX = 0
    private var dragLastY = 0
    private var dragDownAt = 0L

    fun setScreenSize(width: Int, height: Int) {
        if (width > 0 && height > 0) {
            screenW = width
            screenH = height
        }
    }

    fun handlePacket(body: JsonObject) {
        when ((body["t"] as? JsonPrimitive)?.content) {
            "tap" -> handleTap(body)
            "click_abs" -> handleTap(body)
            "button" -> handleButton(body)
            "move" -> handleMove(body)
            "hover" -> handleHover(body)
            "key" -> handleKey(body)
            "type" -> handleType(body)
            "scroll" -> handleScroll(body)
            else -> Unit
        }
    }

    private fun pointerUp(): Boolean {
        return NativePointer.hasBackend()
    }

    private fun pointerMoveTo(nx: Double, ny: Double) {
        val cx = nx.coerceIn(0.0, 1.0)
        val cy = ny.coerceIn(0.0, 1.0)
        lastX = cx
        lastY = cy
        NativePointer.moveAbs(cx, cy, screenW, screenH)
    }

    private fun handleTap(body: JsonObject) {
        if (pointerUp()) {
            val x = (body["x"] as? JsonPrimitive)?.doubleOrNull
            val y = (body["y"] as? JsonPrimitive)?.doubleOrNull
            if (x != null && y != null) {
                pointerMoveTo(x, y)
            }
            NativePointer.button(NativePointer.BTN_LEFT, true)
            NativePointer.button(NativePointer.BTN_LEFT, false)
            return
        }
        val x = (body["x"] as? JsonPrimitive)?.doubleOrNull
        val y = (body["y"] as? JsonPrimitive)?.doubleOrNull
        if (x != null && y != null) {
            tapAt(x, y)
        } else {
            tapAt(lastX, lastY)
        }
    }

    private fun handleButton(body: JsonObject) {
        if (pointerUp()) {
            handlePointerButton(body)
            return
        }
        val down = (body["down"] as? JsonPrimitive)?.content == "true"
        val cancel = (body["cancel"] as? JsonPrimitive)?.content == "true"
        val name = (body["b"] as? JsonPrimitive)?.content
        if (name != "left") {
            if (down && name == "right") {
                val x = (body["x"] as? JsonPrimitive)?.doubleOrNull
                val y = (body["y"] as? JsonPrimitive)?.doubleOrNull
                if (x != null && y != null) {
                    lastX = x
                    lastY = y
                }
                longPressAt(lastX, lastY)
            }
            return
        }
        if (cancel) {
            disarmDrag()
            return
        }
        if (down) {
            val x = (body["x"] as? JsonPrimitive)?.doubleOrNull
            val y = (body["y"] as? JsonPrimitive)?.doubleOrNull
            if (x != null && y != null) {
                lastX = x
                lastY = y
            }
            armDrag(lastX, lastY)
            return
        }
        val x = (body["x"] as? JsonPrimitive)?.doubleOrNull
        val y = (body["y"] as? JsonPrimitive)?.doubleOrNull
        finishDrag(x, y)
    }

    private fun handlePointerButton(body: JsonObject) {
        val down = (body["down"] as? JsonPrimitive)?.content == "true"
        val cancel = (body["cancel"] as? JsonPrimitive)?.content == "true"
        if (cancel) {
            NativePointer.releaseAll()
            return
        }
        val code = when ((body["b"] as? JsonPrimitive)?.content) {
            "left" -> NativePointer.BTN_LEFT
            "right" -> NativePointer.BTN_RIGHT
            "middle" -> NativePointer.BTN_MIDDLE
            else -> null
        }
        if (code == null) {
            return
        }
        val x = (body["x"] as? JsonPrimitive)?.doubleOrNull
        val y = (body["y"] as? JsonPrimitive)?.doubleOrNull
        if (x != null && y != null) {
            pointerMoveTo(x, y)
        }
        NativePointer.button(code, down)
    }

    private fun handleHover(body: JsonObject) {
        val x = (body["x"] as? JsonPrimitive)?.doubleOrNull ?: return
        val y = (body["y"] as? JsonPrimitive)?.doubleOrNull ?: return
        if (!pointerUp()) {
            lastX = x.coerceIn(0.0, 1.0)
            lastY = y.coerceIn(0.0, 1.0)
            return
        }
        pointerMoveTo(x, y)
    }

    private fun handleMove(body: JsonObject) {
        val x = (body["x"] as? JsonPrimitive)?.doubleOrNull ?: return
        val y = (body["y"] as? JsonPrimitive)?.doubleOrNull ?: return
        if (pointerUp()) {
            pointerMoveTo(x, y)
            return
        }
        if (!dragArmed) return
        lastX = x.coerceIn(0.0, 1.0)
        lastY = y.coerceIn(0.0, 1.0)
        dragLastX = (lastX * (screenW - 1)).toInt().coerceIn(0, screenW - 1)
        dragLastY = (lastY * (screenH - 1)).toInt().coerceIn(0, screenH - 1)
        val dx = dragLastX - dragStartX
        val dy = dragLastY - dragStartY
        if (dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
            dragMoved = true
        }
    }

    private fun armDrag(nx: Double, ny: Double) {
        val cx = nx.coerceIn(0.0, 1.0)
        val cy = ny.coerceIn(0.0, 1.0)
        dragArmed = true
        dragMoved = false
        dragStartX = (cx * (screenW - 1)).toInt().coerceIn(0, screenW - 1)
        dragStartY = (cy * (screenH - 1)).toInt().coerceIn(0, screenH - 1)
        dragLastX = dragStartX
        dragLastY = dragStartY
        dragDownAt = System.currentTimeMillis()
    }

    private fun disarmDrag() {
        dragArmed = false
        dragMoved = false
    }

    private fun finishDrag(nx: Double?, ny: Double?) {
        if (!dragArmed) return
        dragArmed = false
        val fx = ((nx ?: lastX).coerceIn(0.0, 1.0) * (screenW - 1)).toInt().coerceIn(0, screenW - 1)
        val fy = ((ny ?: lastY).coerceIn(0.0, 1.0) * (screenH - 1)).toInt().coerceIn(0, screenH - 1)
        lastX = (fx.toDouble() / (screenW - 1).coerceAtLeast(1).toDouble()).coerceIn(0.0, 1.0)
        lastY = (fy.toDouble() / (screenH - 1).coerceAtLeast(1).toDouble()).coerceIn(0.0, 1.0)
        if (!dragMoved) {
            dragMoved = false
            runCommand("input tap " + fx + " " + fy)
            return
        }
        dragMoved = false
        val elapsed = (System.currentTimeMillis() - dragDownAt).coerceIn(120L, 2500L)
        runCommand(
            "input swipe " + dragStartX + " " + dragStartY + " " + fx + " " + fy + " " + elapsed,
        )
    }

    private fun handleKey(body: JsonObject) {
        val down = (body["down"] as? JsonPrimitive)?.content == "true"
        if (!down) return
        val evdev = (body["code"] as? JsonPrimitive)?.intOrNull ?: return
        val androidCode = EVDEV_TO_ANDROID[evdev] ?: return
        runCommand("input keyevent " + androidCode)
    }

    private fun handleType(body: JsonObject) {
        val text = (body["s"] as? JsonPrimitive)?.content ?: return
        if (text.isEmpty()) return
        val clean = text.filter { it in TYPE_ALLOWED }
        if (clean.isEmpty()) return
        runCommand("input text " + clean.replace(" ", "%s"))
    }

    private fun handleScroll(body: JsonObject) {
        val now = System.currentTimeMillis()
        if (now - lastScrollAt < SCROLL_THROTTLE_MS) return
        lastScrollAt = now
        val dx = (body["dx"] as? JsonPrimitive)?.doubleOrNull ?: 0.0
        val dy = (body["dy"] as? JsonPrimitive)?.doubleOrNull ?: 0.0
        if (dx == 0.0 && dy == 0.0) return
        if (pointerUp()) {
            if (dy != 0.0) {
                NativePointer.wheel(-dy.toInt())
            }
            return
        }
        val cx = (lastX * (screenW - 1)).toInt().coerceIn(0, screenW - 1)
        val cy = (lastY * (screenH - 1)).toInt().coerceIn(0, screenH - 1)
        val travel = 200
        val (x1, y1, x2, y2) = if (kotlin.math.abs(dy) >= kotlin.math.abs(dx)) {
            if (dy > 0) {
                Quad(cx, (cy + travel / 2).coerceAtMost(screenH - 1), cx, (cy - travel / 2).coerceAtLeast(0))
            } else {
                Quad(cx, (cy - travel / 2).coerceAtLeast(0), cx, (cy + travel / 2).coerceAtMost(screenH - 1))
            }
        } else {
            if (dx > 0) {
                Quad((cx + travel / 2).coerceAtMost(screenW - 1), cy, (cx - travel / 2).coerceAtLeast(0), cy)
            } else {
                Quad((cx - travel / 2).coerceAtLeast(0), cy, (cx + travel / 2).coerceAtMost(screenW - 1), cy)
            }
        }
        runCommand("input swipe " + x1 + " " + y1 + " " + x2 + " " + y2 + " 150")
    }

    private fun tapAt(nx: Double, ny: Double) {
        val cx = nx.coerceIn(0.0, 1.0)
        val cy = ny.coerceIn(0.0, 1.0)
        lastX = cx
        lastY = cy
        val px = (cx * (screenW - 1)).toInt()
        val py = (cy * (screenH - 1)).toInt()
        runCommand("input tap " + px + " " + py)
    }

    private fun longPressAt(nx: Double, ny: Double) {
        val cx = nx.coerceIn(0.0, 1.0)
        val cy = ny.coerceIn(0.0, 1.0)
        lastX = cx
        lastY = cy
        val px = (cx * (screenW - 1)).toInt()
        val py = (cy * (screenH - 1)).toInt()
        runCommand("input swipe " + px + " " + py + " " + px + " " + py + " 500")
    }

    private fun runCommand(cmd: String) {
        synchronized(lock) {
            if (!writeLocked(cmd)) {
                restartLocked()
                if (!writeLocked(cmd)) {
                    errorCount++
                }
            }
        }
    }

    private fun writeLocked(cmd: String): Boolean {
        return try {
            val out = ensureLocked() ?: return false
            out.write((cmd + "\n").toByteArray(Charsets.UTF_8))
            out.flush()
            true
        } catch (_: IOException) {
            invalidateLocked()
            false
        } catch (_: Exception) {
            invalidateLocked()
            false
        }
    }

    private fun ensureLocked(): OutputStream? {
        val active = proc
        val current = stdin
        if (active != null && current != null) {
            try {
                if (active.isAlive) return current
            } catch (_: Exception) {
            }
            invalidateLocked()
        }
        return try {
            val fresh = ProcessBuilder("su")
                .redirectErrorStream(true)
                .start()
            proc = fresh
            stdin = fresh.outputStream
            fresh.outputStream
        } catch (_: Exception) {
            invalidateLocked()
            null
        }
    }

    private fun restartLocked() {
        invalidateLocked()
        ensureLocked()
    }

    private fun invalidateLocked() {
        stdin = null
        val active = proc
        proc = null
        if (active != null) {
            try {
                active.destroy()
            } catch (_: Exception) {
            }
        }
    }

    private data class Quad(val x1: Int, val y1: Int, val x2: Int, val y2: Int)

    private companion object {
        const val SCROLL_THROTTLE_MS = 120L
        const val DRAG_THRESHOLD_PX = 16
        const val TYPE_ALLOWED = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,?!_-'\""

        val EVDEV_TO_ANDROID: Map<Int, Int> = buildMap {
            put(1, KeyEvent.KEYCODE_ESCAPE)
            put(14, KeyEvent.KEYCODE_DEL)
            put(15, KeyEvent.KEYCODE_TAB)
            put(28, KeyEvent.KEYCODE_ENTER)
            put(29, KeyEvent.KEYCODE_CTRL_LEFT)
            put(56, KeyEvent.KEYCODE_ALT_LEFT)
            put(57, KeyEvent.KEYCODE_SPACE)
            put(102, KeyEvent.KEYCODE_HOME)
            put(103, KeyEvent.KEYCODE_DPAD_UP)
            put(104, KeyEvent.KEYCODE_PAGE_UP)
            put(105, KeyEvent.KEYCODE_DPAD_LEFT)
            put(106, KeyEvent.KEYCODE_DPAD_RIGHT)
            put(107, KeyEvent.KEYCODE_MOVE_END)
            put(108, KeyEvent.KEYCODE_DPAD_DOWN)
            put(109, KeyEvent.KEYCODE_PAGE_DOWN)
            put(2, KeyEvent.KEYCODE_1)
            put(3, KeyEvent.KEYCODE_2)
            put(4, KeyEvent.KEYCODE_3)
            put(5, KeyEvent.KEYCODE_4)
            put(6, KeyEvent.KEYCODE_5)
            put(7, KeyEvent.KEYCODE_6)
            put(8, KeyEvent.KEYCODE_7)
            put(9, KeyEvent.KEYCODE_8)
            put(10, KeyEvent.KEYCODE_9)
            put(11, KeyEvent.KEYCODE_0)
            put(59, KeyEvent.KEYCODE_F1)
            put(60, KeyEvent.KEYCODE_F2)
            put(61, KeyEvent.KEYCODE_F3)
            put(62, KeyEvent.KEYCODE_F4)
            put(63, KeyEvent.KEYCODE_F5)
            put(64, KeyEvent.KEYCODE_F6)
            put(65, KeyEvent.KEYCODE_F7)
            put(66, KeyEvent.KEYCODE_F8)
            put(67, KeyEvent.KEYCODE_F9)
            put(68, KeyEvent.KEYCODE_F10)
            put(87, KeyEvent.KEYCODE_F11)
            put(88, KeyEvent.KEYCODE_F12)
            val letters = listOf(
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
            )
            for ((letter, evdev) in letters) {
                put(evdev, KeyEvent.KEYCODE_A + (letter - 'a'))
            }
        }
    }
}
