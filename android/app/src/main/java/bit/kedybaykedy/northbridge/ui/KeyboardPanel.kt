package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.Evdev
import bit.kedybaykedy.northbridge.transport.ConnectionState
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import bit.kedybaykedy.northbridge.ui.theme.MmInk
import bit.kedybaykedy.northbridge.ui.theme.MmKicker
import bit.kedybaykedy.northbridge.ui.theme.MmMeta
import bit.kedybaykedy.northbridge.ui.theme.MmPanel
import bit.kedybaykedy.northbridge.ui.theme.MmRaised
import bit.kedybaykedy.northbridge.ui.theme.MmShadow
import kotlin.math.tan

private const val SKEW_DEGREES = -10f

private data class SpecialKey(val label: String, val code: Int)

private val toolbarSpecials = listOf(
    SpecialKey("ESC", Evdev.KEY_ESC),
    SpecialKey("TAB", Evdev.KEY_TAB),
    SpecialKey("←", Evdev.KEY_LEFT),
    SpecialKey("↑", Evdev.KEY_UP),
    SpecialKey("↓", Evdev.KEY_DOWN),
    SpecialKey("→", Evdev.KEY_RIGHT),
    SpecialKey("HOME", Evdev.KEY_HOME),
    SpecialKey("END", Evdev.KEY_END),
    SpecialKey("PGUP", Evdev.KEY_PAGEUP),
    SpecialKey("PGDN", Evdev.KEY_PAGEDOWN),
    SpecialKey("ENTER", Evdev.KEY_ENTER),
    SpecialKey("BKSP", Evdev.KEY_BACKSPACE),
)

private val numberRow = listOf('1', '2', '3', '4', '5', '6', '7', '8', '9', '0')
private val topRow = listOf('q', 'w', 'e', 'r', 't', 'y', 'u', 'ı', 'o', 'p', 'ğ', 'ü')
private val homeRow = listOf('a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'ş', 'i')
private val bottomRow = listOf('z', 'x', 'c', 'v', 'b', 'n', 'm', 'ö', 'ç')

@Composable
fun KeyboardPanel(viewModel: TouchpadViewModel, modifier: Modifier = Modifier) {
    val ctrlLatched by viewModel.ctrlLatched.collectAsState()
    val altLatched by viewModel.altLatched.collectAsState()
    val connection by AppGraph.inputChannel.state.collectAsState()

    Box(
        modifier = modifier
            .fillMaxWidth()
            .background(MmPanel)
            .border(width = 2.dp, color = MmInk),
    ) {
        Canvas(Modifier.matchParentSize()) {
            val step = 14.dp.toPx()
            val radius = step * 0.185f
            val dot = MmInk.copy(alpha = 0.07f)
            var row = 0
            var y = 0f
            while (y <= size.height) {
                var x = if (row % 2 == 1) step / 2f else 0f
                while (x <= size.width) {
                    drawCircle(color = dot, radius = radius, center = Offset(x, y))
                    x += step
                }
                y += step
                row += 1
            }
        }
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = "KLAVYE · TR-Q",
                style = TextStyle(
                    fontFamily = ArchivoBlackFamily,
                    fontSize = 11.sp,
                    letterSpacing = 3.sp,
                    color = MmKicker,
                ),
            )
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MmLatchKey(
                    label = "CTRL",
                    latched = ctrlLatched,
                    onTap = viewModel::toggleCtrl,
                )
                MmLatchKey(
                    label = "ALT",
                    latched = altLatched,
                    onTap = viewModel::toggleAlt,
                )
                for (special in toolbarSpecials) {
                    MmKey(
                        label = special.label,
                        onTap = { viewModel.tapSpecial(special.code) },
                        modifier = Modifier.widthIn(min = 52.dp),
                        minHeight = 44.dp,
                        labelSize = 10.sp,
                    )
                }
            }
            DoubleRule()
            KeyboardRow(keys = numberRow, onChar = viewModel::typeChar)
            KeyboardRow(keys = topRow, onChar = viewModel::typeChar)
            KeyboardRow(keys = homeRow, onChar = viewModel::typeChar)
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Spacer(Modifier.weight(0.5f))
                for (char in bottomRow) {
                    MmKey(
                        label = char.toString(),
                        onTap = { viewModel.typeChar(char) },
                        modifier = Modifier.weight(1f),
                        minHeight = 48.dp,
                        labelSize = 13.sp,
                    )
                }
                Spacer(Modifier.weight(0.5f))
            }
            MmKey(
                label = "SPACE",
                onTap = { viewModel.typeChar(' ') },
                modifier = Modifier.fillMaxWidth(),
                minHeight = 48.dp,
                labelSize = 11.sp,
                labelFamily = CourierPrimeFamily,
            )
            Text(
                text = panelStatus(connection, ctrlLatched, altLatched),
                style = TextStyle(
                    fontFamily = CourierPrimeFamily,
                    fontSize = 11.sp,
                    color = MmMeta,
                ),
            )
        }
    }
}

@Composable
private fun KeyboardRow(
    keys: List<Char>,
    onChar: (Char) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (char in keys) {
            MmKey(
                label = char.toString(),
                onTap = { onChar(char) },
                modifier = Modifier.weight(1f),
                minHeight = 48.dp,
                labelSize = 13.sp,
            )
        }
    }
}

@Composable
private fun MmLatchKey(label: String, latched: Boolean, onTap: () -> Unit) {
    MmKey(
        label = label,
        onTap = onTap,
        modifier = Modifier.widthIn(min = 56.dp),
        minHeight = 44.dp,
        labelSize = 10.sp,
        latched = latched,
    )
}

@Composable
private fun MmKey(
    label: String,
    onTap: () -> Unit,
    modifier: Modifier = Modifier,
    minHeight: androidx.compose.ui.unit.Dp = 48.dp,
    labelSize: androidx.compose.ui.unit.TextUnit = 13.sp,
    labelFamily: FontFamily = ArchivoBlackFamily,
    latched: Boolean = false,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val active = pressed || latched
    val face = if (active) MmInk else MmRaised
    val ink = if (active) MmPanel else MmInk
    Box(
        modifier = modifier
            .skewX(SKEW_DEGREES)
            .heightIn(min = minHeight)
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onTap,
            ),
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
                modifier = Modifier.skewX(-SKEW_DEGREES),
                style = TextStyle(
                    fontFamily = labelFamily,
                    fontSize = labelSize,
                    color = ink,
                ),
            )
        }
    }
}

@Composable
private fun DoubleRule() {
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(3.dp)
                .background(MmInk),
        )
        Spacer(Modifier.height(2.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(MmInk),
        )
    }
}

private fun panelStatus(
    connection: ConnectionState,
    ctrlLatched: Boolean,
    altLatched: Boolean,
): String {
    val base = when (connection) {
        ConnectionState.CONNECTED -> "READY"
        ConnectionState.CONNECTING -> "CONNECTING"
        ConnectionState.RECONNECTING -> "RECONNECTING"
        ConnectionState.DISCONNECTED -> "NO LINK"
    }
    val latches = buildString {
        if (ctrlLatched) append(" · CTRL")
        if (altLatched) append(" · ALT")
    }
    return base + latches
}
