package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Keyboard
import androidx.compose.material.icons.filled.KeyboardHide
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.touchpadGestures
import bit.kedybaykedy.northbridge.transport.ConnectionState
import bit.kedybaykedy.northbridge.transport.MouseButton
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import bit.kedybaykedy.northbridge.ui.theme.MmInk
import bit.kedybaykedy.northbridge.ui.theme.MmKicker
import bit.kedybaykedy.northbridge.ui.theme.MmMeta
import bit.kedybaykedy.northbridge.ui.theme.MmPanel
import bit.kedybaykedy.northbridge.ui.theme.MmRaised
import bit.kedybaykedy.northbridge.ui.theme.MmShadow

@Composable
fun TouchpadScreen(viewModel: TouchpadViewModel = viewModel()) {
    val connection by AppGraph.inputChannel.state.collectAsState()
    val keyboardOpen by viewModel.keyboardOpen.collectAsState()

    Column(
        Modifier
            .fillMaxSize()
            .background(MmPanel),
    ) {
        ConnectionBanner(connection)
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .background(MmPanel)
                .touchpadGestures(viewModel::send)
                .padding(12.dp),
            contentAlignment = Alignment.Center,
        ) {
            if (connection != ConnectionState.CONNECTED) {
                Text(
                    text = if (connection == ConnectionState.DISCONNECTED) {
                        "NO ACTIVE DEVICE — PAIR ONE IN DEVICES"
                    } else {
                        "WAITING FOR CONNECTION…"
                    },
                    style = TextStyle(
                        fontFamily = CourierPrimeFamily,
                        fontSize = 12.sp,
                        color = MmMeta,
                    ),
                )
            } else {
                Text(
                    text = "TOUCHPAD · READY",
                    style = TextStyle(
                        fontFamily = ArchivoBlackFamily,
                        fontSize = 11.sp,
                        letterSpacing = 3.sp,
                        color = MmKicker,
                    ),
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MouseButtonChip("L", MouseButton.LEFT, viewModel, Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            MouseButtonChip("R", MouseButton.RIGHT, viewModel, Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            MouseButtonChip("M", MouseButton.MIDDLE, viewModel, Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            IconButton(onClick = viewModel::toggleKeyboard) {
                Icon(
                    if (keyboardOpen) Icons.Filled.KeyboardHide else Icons.Filled.Keyboard,
                    contentDescription = "Keyboard",
                    tint = MmInk,
                )
            }
        }
        if (keyboardOpen) {
            KeyboardPanel(
                viewModel = viewModel,
                modifier = Modifier.padding(horizontal = 12.dp),
            )
            Spacer(Modifier.height(12.dp))
        }
    }
}

@Composable
private fun MouseButtonChip(
    label: String,
    button: MouseButton,
    viewModel: TouchpadViewModel,
    modifier: Modifier = Modifier,
) {
    var held by remember { mutableStateOf(false) }
    val face = if (held) MmInk else MmRaised
    val ink = if (held) MmPanel else MmInk
    Box(
        modifier = modifier
            .skewX(-10f)
            .heightIn(min = 44.dp)
            .pointerInput(button) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    held = true
                    viewModel.pressButton(button, true)
                    waitForUpOrCancellation()
                    held = false
                    viewModel.pressButton(button, false)
                }
            },
    ) {
        Box(
            Modifier
                .matchParentSize()
                .offset(x = 4.dp, y = 4.dp)
                .background(MmShadow),
        )
        Box(
            Modifier
                .matchParentSize()
                .background(face)
                .border(width = 1.5.dp, color = MmInk),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = label,
                modifier = Modifier.skewX(10f),
                style = TextStyle(
                    fontFamily = ArchivoBlackFamily,
                    fontSize = 12.sp,
                    color = ink,
                ),
            )
        }
    }
}
