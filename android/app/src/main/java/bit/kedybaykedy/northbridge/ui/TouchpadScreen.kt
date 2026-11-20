package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Keyboard
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.Evdev
import bit.kedybaykedy.northbridge.input.touchpadGestures
import bit.kedybaykedy.northbridge.transport.ConnectionState
import bit.kedybaykedy.northbridge.transport.MouseButton

@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun TouchpadScreen(viewModel: TouchpadViewModel = viewModel()) {
    val connection by AppGraph.inputChannel.state.collectAsState()
    val ctrlLatched by viewModel.ctrlLatched.collectAsState()
    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    var keyboardText by remember { mutableStateOf("") }

    Column(Modifier.fillMaxSize()) {
        ConnectionBanner(connection)
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .background(Color(0xFF14171B))
                .touchpadGestures(viewModel::send),
            contentAlignment = Alignment.Center,
        ) {
            if (connection != ConnectionState.CONNECTED) {
                Text(
                    text = if (connection == ConnectionState.DISCONNECTED) {
                        "No active device — pair one in Devices"
                    } else {
                        "Waiting for connection…"
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(
                modifier = Modifier
                    .weight(1f)
                    .horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MouseButtonChip("L", MouseButton.LEFT, viewModel)
                MouseButtonChip("R", MouseButton.RIGHT, viewModel)
                MouseButtonChip("M", MouseButton.MIDDLE, viewModel)
                KeyChip("Esc") { viewModel.tapKey(Evdev.KEY_ESC) }
                KeyChip("Tab") { viewModel.tapKey(Evdev.KEY_TAB) }
                KeyChip("Enter") { viewModel.tapKey(Evdev.KEY_ENTER) }
                KeyChip("Ctrl", active = ctrlLatched, onClick = viewModel::toggleCtrl)
            }
            IconButton(
                onClick = {
                    focusRequester.requestFocus()
                    keyboardController?.show()
                },
            ) {
                Icon(Icons.Filled.Keyboard, contentDescription = "Keyboard")
            }
        }
        BasicTextField(
            value = keyboardText,
            onValueChange = { value ->
                if (value.isNotEmpty()) {
                    viewModel.submitText(value)
                    keyboardText = ""
                }
            },
            modifier = Modifier
                .size(1.dp)
                .focusRequester(focusRequester),
            textStyle = TextStyle(color = Color.Transparent, fontSize = 1.sp),
            cursorBrush = SolidColor(Color.Transparent),
        )
    }
}

@Composable
private fun MouseButtonChip(label: String, button: MouseButton, viewModel: TouchpadViewModel) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(MaterialTheme.colorScheme.secondaryContainer)
            .pointerInput(button) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    viewModel.pressButton(button, true)
                    waitForUpOrCancellation()
                    viewModel.pressButton(button, false)
                }
            }
            .padding(horizontal = 14.dp, vertical = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSecondaryContainer,
        )
    }
}

@Composable
private fun KeyChip(label: String, active: Boolean = false, onClick: () -> Unit) {
    val background = if (active) {
        MaterialTheme.colorScheme.primary
    } else {
        MaterialTheme.colorScheme.surfaceVariant
    }
    val foreground = if (active) {
        MaterialTheme.colorScheme.onPrimary
    } else {
        MaterialTheme.colorScheme.onSurfaceVariant
    }
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(background)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = foreground,
        )
    }
}
