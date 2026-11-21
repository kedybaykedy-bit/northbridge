package bit.kedybaykedy.northbridge.input

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import bit.kedybaykedy.northbridge.transport.InputMsg
import bit.kedybaykedy.northbridge.transport.MouseButton
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.hypot

private const val TAP_MAX_MS = 300L
private const val DOUBLE_TAP_WINDOW_MS = 300L
private const val MOVE_INTERVAL_MS = 16L

const val UI_TOGGLE_ID = "ui_toggle"

@Composable
fun Modifier.touchpadGestures(send: (InputMsg) -> Unit): Modifier {
    val currentSend by rememberUpdatedState(send)
    return this.pointerInput(Unit) {
        coroutineScope {
            var pendingTap: Job? = null
            var lastTapAt = 0L

            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val startedAt = System.currentTimeMillis()
                val startPos = down.position
                val pointerPositions = mutableMapOf<PointerId, Offset>()
                pointerPositions[down.id] = down.position

                var moveAccX = 0f
                var moveAccY = 0f
                var scrollAccX = 0f
                var scrollAccY = 0f
                var lastSentAt = 0L
                var maxTravel = 0f
                var gestureMoved = false
                var twoFinger = false
                var twoFingerSince = 0L
                var twoFingerTravel = 0f

                while (true) {
                    val event = awaitPointerEvent()
                    val pressed = event.changes.filter { it.pressed }
                    if (pressed.isEmpty()) break

                    val now = System.currentTimeMillis()

                    if (pressed.size >= 2) {
                        if (!twoFinger) {
                            if (moveAccX != 0f || moveAccY != 0f) {
                                currentSend(
                                    InputMsg.RelMove(moveAccX.toDouble(), moveAccY.toDouble()),
                                )
                                moveAccX = 0f
                                moveAccY = 0f
                            }
                            twoFinger = true
                            twoFingerSince = now
                            twoFingerTravel = 0f
                        }
                        var dx = 0f
                        var dy = 0f
                        for (change in pressed) {
                            val previous =
                                pointerPositions.getOrPut(change.id) { change.previousPosition }
                            val delta = change.position - previous
                            pointerPositions[change.id] = change.position
                            dx += delta.x
                            dy += delta.y
                            twoFingerTravel += hypot(delta.x, delta.y)
                        }
                        if (twoFingerTravel > viewConfiguration.touchSlop) {
                            scrollAccX += dx / pressed.size
                            scrollAccY += dy / pressed.size
                            if (now - lastSentAt >= MOVE_INTERVAL_MS &&
                                (scrollAccX != 0f || scrollAccY != 0f)
                            ) {
                                currentSend(
                                    InputMsg.Scroll(scrollAccX.toDouble(), scrollAccY.toDouble()),
                                )
                                scrollAccX = 0f
                                scrollAccY = 0f
                                lastSentAt = now
                            }
                        }
                    } else {
                        val change = pressed.first()
                        val previous =
                            pointerPositions.getOrPut(change.id) { change.previousPosition }
                        val delta = change.position - previous
                        pointerPositions[change.id] = change.position
                        if (!twoFinger) {
                            val travel = hypot(
                                change.position.x - startPos.x,
                                change.position.y - startPos.y,
                            )
                            if (travel > maxTravel) maxTravel = travel
                            if (!gestureMoved && maxTravel > viewConfiguration.touchSlop) {
                                gestureMoved = true
                                pendingTap?.cancel()
                                pendingTap = null
                            }
                            if (gestureMoved) {
                                moveAccX += delta.x
                                moveAccY += delta.y
                                if (now - lastSentAt >= MOVE_INTERVAL_MS &&
                                    (moveAccX != 0f || moveAccY != 0f)
                                ) {
                                    currentSend(
                                        InputMsg.RelMove(moveAccX.toDouble(), moveAccY.toDouble()),
                                    )
                                    moveAccX = 0f
                                    moveAccY = 0f
                                    lastSentAt = now
                                }
                            }
                        }
                    }
                }

                if (moveAccX != 0f || moveAccY != 0f) {
                    currentSend(InputMsg.RelMove(moveAccX.toDouble(), moveAccY.toDouble()))
                }
                if (scrollAccX != 0f || scrollAccY != 0f) {
                    currentSend(InputMsg.Scroll(scrollAccX.toDouble(), scrollAccY.toDouble()))
                }

                val endedAt = System.currentTimeMillis()
                val duration = endedAt - startedAt
                if (twoFinger) {
                    val quickTogether = endedAt - twoFingerSince <= TAP_MAX_MS
                    val quickStart = twoFingerSince - startedAt <= TAP_MAX_MS
                    if (twoFingerTravel <= viewConfiguration.touchSlop &&
                        duration <= TAP_MAX_MS * 2 &&
                        quickTogether &&
                        quickStart
                    ) {
                        currentSend(InputMsg.Tap(MouseButton.RIGHT, 1))
                    }
                } else if (!gestureMoved &&
                    maxTravel <= viewConfiguration.touchSlop &&
                    duration <= TAP_MAX_MS
                ) {
                    val previousTap = pendingTap
                    if (previousTap != null && endedAt - lastTapAt <= DOUBLE_TAP_WINDOW_MS) {
                        previousTap.cancel()
                        pendingTap = null
                        lastTapAt = 0L
                        currentSend(InputMsg.Tap(MouseButton.LEFT, 2))
                    } else {
                        lastTapAt = endedAt
                        pendingTap = launch {
                            delay(DOUBLE_TAP_WINDOW_MS)
                            pendingTap = null
                            currentSend(InputMsg.Tap(MouseButton.LEFT, 1))
                        }
                    }
                }
            }
        }
    }
}

class KeyZoneMap {
    internal val layouts = mutableMapOf<String, LayoutCoordinates>()
    internal var surface: LayoutCoordinates? = null

    fun hitTest(position: Offset): String? {
        val root = surface ?: return null
        if (!root.isAttached) return null
        val ordered = layouts.entries.toList()
        for (index in ordered.size - 1 downTo 0) {
            val (id, child) = ordered[index]
            if (!child.isAttached) continue
            val origin = try {
                root.localPositionOf(child, Offset.Zero)
            } catch (_: IllegalStateException) {
                continue
            } catch (_: IllegalArgumentException) {
                continue
            }
            val rect = Rect(
                origin,
                Size(child.size.width.toFloat(), child.size.height.toFloat()),
            )
            if (rect.contains(position)) return id
        }
        return null
    }
}

fun Modifier.unifiedSurfaceRoot(zones: KeyZoneMap): Modifier =
    this.onGloballyPositioned { zones.surface = it }

fun Modifier.keyZone(zones: KeyZoneMap, id: String): Modifier =
    this.onGloballyPositioned { zones.layouts[id] = it }

interface UnifiedKeyHost {
    fun isHoldable(id: String): Boolean
    fun isRepeatable(id: String): Boolean
    fun onTap(id: String)
    fun onHoldStart(id: String)
    fun onHoldEnd(id: String)
    fun onRepeat(id: String)
    fun onPressedChange(id: String?)
}

private const val HOLD_START_MS = 400L
private const val REPEAT_START_MS = 450L
private const val REPEAT_INTERVAL_MS = 55L

@Composable
fun Modifier.unifiedInputGestures(
    zones: KeyZoneMap,
    host: UnifiedKeyHost,
    send: (InputMsg) -> Unit,
): Modifier {
    val currentSend by rememberUpdatedState(send)
    val currentHost by rememberUpdatedState(host)
    return this.pointerInput(zones) {
        coroutineScope {
            var pendingTap: Job? = null
            var lastTapAt = 0L

            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val startedAt = System.currentTimeMillis()
                val startPos = down.position
                val pointerPositions = mutableMapOf<PointerId, Offset>()
                pointerPositions[down.id] = down.position

                var keyId: String? = zones.hitTest(down.position)
                currentHost.onPressedChange(keyId)

                if (keyId == UI_TOGGLE_ID) {
                    consumeUiToggleTap(currentHost, keyId, startPos)
                } else {
                var moveAccX = 0f
                var moveAccY = 0f
                var scrollAccX = 0f
                var scrollAccY = 0f
                var lastSentAt = 0L
                var maxTravel = 0f
                var gestureMoved = false
                var twoFinger = false
                var twoFingerSince = 0L
                var twoFingerTravel = 0f
                var holdFired = false

                var holdJob: Job? = null
                var repeatJob: Job? = null
                fun cancelKeyJobs() {
                    holdJob?.cancel()
                    holdJob = null
                    repeatJob?.cancel()
                    repeatJob = null
                }
                val heldId = keyId
                if (heldId != null) {
                    if (currentHost.isHoldable(heldId)) {
                        holdJob = launch {
                            delay(HOLD_START_MS)
                            currentHost.onHoldStart(heldId)
                            holdFired = true
                        }
                    } else if (currentHost.isRepeatable(heldId)) {
                        repeatJob = launch {
                            delay(REPEAT_START_MS)
                            while (true) {
                                currentHost.onRepeat(heldId)
                                delay(REPEAT_INTERVAL_MS)
                            }
                        }
                    }
                }

                while (true) {
                    val event = awaitPointerEvent()
                    val pressed = event.changes.filter { it.pressed }
                    if (pressed.isEmpty()) break

                    val now = System.currentTimeMillis()

                    if (pressed.size >= 2) {
                        if (!twoFinger) {
                            if (moveAccX != 0f || moveAccY != 0f) {
                                currentSend(
                                    InputMsg.RelMove(moveAccX.toDouble(), moveAccY.toDouble()),
                                )
                                moveAccX = 0f
                                moveAccY = 0f
                            }
                            twoFinger = true
                            twoFingerSince = now
                            twoFingerTravel = 0f
                            cancelKeyJobs()
                            keyId = null
                            currentHost.onPressedChange(null)
                        }
                        var dx = 0f
                        var dy = 0f
                        for (change in pressed) {
                            val previous =
                                pointerPositions.getOrPut(change.id) { change.previousPosition }
                            val delta = change.position - previous
                            pointerPositions[change.id] = change.position
                            dx += delta.x
                            dy += delta.y
                            twoFingerTravel += hypot(delta.x, delta.y)
                        }
                        if (twoFingerTravel > viewConfiguration.touchSlop) {
                            scrollAccX += dx / pressed.size
                            scrollAccY += dy / pressed.size
                            if (now - lastSentAt >= MOVE_INTERVAL_MS &&
                                (scrollAccX != 0f || scrollAccY != 0f)
                            ) {
                                currentSend(
                                    InputMsg.Scroll(scrollAccX.toDouble(), scrollAccY.toDouble()),
                                )
                                scrollAccX = 0f
                                scrollAccY = 0f
                                lastSentAt = now
                            }
                        }
                    } else {
                        val change = pressed.first()
                        val previous =
                            pointerPositions.getOrPut(change.id) { change.previousPosition }
                        val delta = change.position - previous
                        pointerPositions[change.id] = change.position
                        if (!twoFinger) {
                            val travel = hypot(
                                change.position.x - startPos.x,
                                change.position.y - startPos.y,
                            )
                            if (travel > maxTravel) maxTravel = travel
                            if (!gestureMoved && maxTravel > viewConfiguration.touchSlop) {
                                gestureMoved = true
                                pendingTap?.cancel()
                                pendingTap = null
                                cancelKeyJobs()
                                keyId = null
                                currentHost.onPressedChange(null)
                            }
                            if (gestureMoved) {
                                moveAccX += delta.x
                                moveAccY += delta.y
                                if (now - lastSentAt >= MOVE_INTERVAL_MS &&
                                    (moveAccX != 0f || moveAccY != 0f)
                                ) {
                                    currentSend(
                                        InputMsg.RelMove(moveAccX.toDouble(), moveAccY.toDouble()),
                                    )
                                    moveAccX = 0f
                                    moveAccY = 0f
                                    lastSentAt = now
                                }
                            }
                        }
                    }
                }

                cancelKeyJobs()

                if (moveAccX != 0f || moveAccY != 0f) {
                    currentSend(InputMsg.RelMove(moveAccX.toDouble(), moveAccY.toDouble()))
                }
                if (scrollAccX != 0f || scrollAccY != 0f) {
                    currentSend(InputMsg.Scroll(scrollAccX.toDouble(), scrollAccY.toDouble()))
                }

                val endedAt = System.currentTimeMillis()
                val duration = endedAt - startedAt
                val tappedKey = keyId
                currentHost.onPressedChange(null)
                if (twoFinger) {
                    val quickTogether = endedAt - twoFingerSince <= TAP_MAX_MS
                    val quickStart = twoFingerSince - startedAt <= TAP_MAX_MS
                    if (twoFingerTravel <= viewConfiguration.touchSlop &&
                        duration <= TAP_MAX_MS * 2 &&
                        quickTogether &&
                        quickStart
                    ) {
                        currentSend(InputMsg.Tap(MouseButton.RIGHT, 1))
                    }
                } else if (!gestureMoved && maxTravel <= viewConfiguration.touchSlop) {
                    if (tappedKey != null) {
                        if (holdFired) {
                            currentHost.onHoldEnd(tappedKey)
                        } else {
                            currentHost.onTap(tappedKey)
                        }
                    } else if (duration <= TAP_MAX_MS) {
                        val previousTap = pendingTap
                        if (previousTap != null && endedAt - lastTapAt <= DOUBLE_TAP_WINDOW_MS) {
                            previousTap.cancel()
                            pendingTap = null
                            lastTapAt = 0L
                            currentSend(InputMsg.Tap(MouseButton.LEFT, 2))
                        } else {
                            lastTapAt = endedAt
                            pendingTap = launch {
                                delay(DOUBLE_TAP_WINDOW_MS)
                                pendingTap = null
                                currentSend(InputMsg.Tap(MouseButton.LEFT, 1))
                            }
                        }
                    }
                } else {
                    val releasedHold = if (holdFired) heldId else null
                    if (releasedHold != null) currentHost.onHoldEnd(releasedHold)
                }
                }
            }
        }
    }
}

private suspend fun AwaitPointerEventScope.consumeUiToggleTap(
    host: UnifiedKeyHost,
    keyId: String,
    anchor: Offset,
) {
    var clean = true
    var peak = 0f
    while (true) {
        val event = awaitPointerEvent()
        val pressed = event.changes.filter { it.pressed }
        if (pressed.isEmpty()) break
        if (pressed.size > 1) clean = false
        val first = pressed[0]
        val travel = hypot(first.position.x - anchor.x, first.position.y - anchor.y)
        if (travel > peak) peak = travel
        if (peak > viewConfiguration.touchSlop) clean = false
    }
    host.onPressedChange(null)
    if (clean) host.onTap(keyId)
}

@Composable
fun Modifier.keyPressGestures(
    zones: KeyZoneMap,
    host: UnifiedKeyHost,
): Modifier {
    val currentHost by rememberUpdatedState(host)
    return this.pointerInput(zones) {
        coroutineScope {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val tappedKey: String? = zones.hitTest(down.position)
                currentHost.onPressedChange(tappedKey)
                var holdFired = false
                var holdJob: Job? = null
                var repeatJob: Job? = null
                if (tappedKey != null) {
                    if (currentHost.isHoldable(tappedKey)) {
                        holdJob = launch {
                            delay(HOLD_START_MS)
                            currentHost.onHoldStart(tappedKey)
                            holdFired = true
                        }
                    } else if (currentHost.isRepeatable(tappedKey)) {
                        repeatJob = launch {
                            delay(REPEAT_START_MS)
                            while (true) {
                                currentHost.onRepeat(tappedKey)
                                delay(REPEAT_INTERVAL_MS)
                            }
                        }
                    }
                }
                while (true) {
                    val event = awaitPointerEvent()
                    val pressed = event.changes.filter { it.pressed }
                    if (pressed.isEmpty()) break
                }
                holdJob?.cancel()
                repeatJob?.cancel()
                currentHost.onPressedChange(null)
                if (tappedKey != null) {
                    if (holdFired) {
                        currentHost.onHoldEnd(tappedKey)
                    } else {
                        currentHost.onTap(tappedKey)
                    }
                }
            }
        }
    }
}
