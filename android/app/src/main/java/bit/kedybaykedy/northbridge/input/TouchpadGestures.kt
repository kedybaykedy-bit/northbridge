package bit.kedybaykedy.northbridge.input

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
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
