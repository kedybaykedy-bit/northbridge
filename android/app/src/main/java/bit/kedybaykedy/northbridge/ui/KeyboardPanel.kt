package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.Evdev
import bit.kedybaykedy.northbridge.input.KeyZoneMap
import bit.kedybaykedy.northbridge.input.UI_TOGGLE_ID
import bit.kedybaykedy.northbridge.input.UnifiedKeyHost
import bit.kedybaykedy.northbridge.input.keyPressGestures
import bit.kedybaykedy.northbridge.input.keyZone
import bit.kedybaykedy.northbridge.input.unifiedInputGestures
import bit.kedybaykedy.northbridge.input.unifiedSurfaceRoot
import bit.kedybaykedy.northbridge.transport.ConnectionState
import bit.kedybaykedy.northbridge.transport.MouseButton
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import bit.kedybaykedy.northbridge.ui.theme.MmInk
import bit.kedybaykedy.northbridge.ui.theme.MmMeta
import bit.kedybaykedy.northbridge.ui.theme.MmPanel
import bit.kedybaykedy.northbridge.ui.theme.MmRaised
import bit.kedybaykedy.northbridge.ui.theme.MmShadow

private const val SKEW_DEGREES = -10f

private val digitChars = listOf('1', '2', '3', '4', '5', '6', '7', '8', '9', '0')
private val symbolChars = listOf('-', '/', ':', ';', '(', ')', '&', '@', '"', '!')

private val topRow = listOf('q', 'w', 'e', 'r', 't', 'y', 'u', 'ı', 'o', 'p', 'ğ', 'ü')
private val homeRow = listOf('a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'ş', 'i')
private val bottomRow = listOf('z', 'x', 'c', 'v', 'b', 'n', 'm', 'ö', 'ç')

private const val ID_MOUSE_LEFT = "mouse_left"
private const val ID_MOUSE_RIGHT = "mouse_right"
private const val ID_MOUSE_MIDDLE = "mouse_middle"
private const val ID_LATCH_CTRL = "latch_ctrl"
private const val ID_LATCH_ALT = "latch_alt"
private const val ID_LATCH_SHIFT = "latch_shift"
private const val ID_LATCH_SYM = "latch_sym"
private const val ID_ESC = "special_esc"
private const val ID_TAB = "special_tab"
private const val ID_HOME = "special_home"
private const val ID_END = "special_end"
private const val ID_PGUP = "special_pgup"
private const val ID_PGDN = "special_pgdn"
private const val ID_ENTER = "special_enter"
private const val ID_BKSP = "special_bksp"
private const val ID_DEL = "special_del"
private const val ID_UP = "special_up"
private const val ID_DOWN = "special_down"
private const val ID_LEFT = "special_left"
private const val ID_RIGHT = "special_right"
private const val ID_SPACE = "space"
private const val ID_COMMA = "char_comma"
private const val ID_DOT = "char_dot"

private fun digitId(index: Int) = "digit_$index"
private fun charId(char: Char) = "char_$char"

@Composable
fun KeyboardPanel(viewModel: TouchpadViewModel, modifier: Modifier = Modifier) {
    val ctrlLatched by viewModel.ctrlLatched.collectAsState()
    val altLatched by viewModel.altLatched.collectAsState()
    val shiftLatched by viewModel.shiftLatched.collectAsState()
    val symLatched by viewModel.symLatched.collectAsState()
    val connection by AppGraph.inputChannel.state.collectAsState()

    val zones = remember { KeyZoneMap() }
    var pressedId by remember { mutableStateOf<String?>(null) }

    val host = remember(viewModel) {
        object : UnifiedKeyHost {
            override fun isHoldable(id: String): Boolean = id.startsWith("mouse_")

            override fun isRepeatable(id: String): Boolean =
                id.startsWith("char_") ||
                    id.startsWith("digit_") ||
                    id == ID_SPACE ||
                    id == ID_BKSP ||
                    id == ID_DEL ||
                    id == ID_ENTER ||
                    id == ID_UP ||
                    id == ID_DOWN ||
                    id == ID_LEFT ||
                    id == ID_RIGHT

            override fun onTap(id: String) = fire(id, viewModel)

            override fun onHoldStart(id: String) {
                when (id) {
                    ID_MOUSE_LEFT -> viewModel.pressButton(MouseButton.LEFT, true)
                    ID_MOUSE_RIGHT -> viewModel.pressButton(MouseButton.RIGHT, true)
                    ID_MOUSE_MIDDLE -> viewModel.pressButton(MouseButton.MIDDLE, true)
                }
            }

            override fun onHoldEnd(id: String) {
                when (id) {
                    ID_MOUSE_LEFT -> viewModel.pressButton(MouseButton.LEFT, false)
                    ID_MOUSE_RIGHT -> viewModel.pressButton(MouseButton.RIGHT, false)
                    ID_MOUSE_MIDDLE -> viewModel.pressButton(MouseButton.MIDDLE, false)
                }
            }

            override fun onRepeat(id: String) = fire(id, viewModel)

            override fun onPressedChange(id: String?) {
                pressedId = id
            }
        }
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .background(MmPanel)
            .border(width = 2.dp, color = MmInk)
            .unifiedSurfaceRoot(zones)
            .keyPressGestures(zones, host),
    ) {
        HalftoneDots(Modifier.matchParentSize())
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            PortraitMouseRow(zones, pressedId)
            PortraitDigitRow(zones, pressedId, symLatched)
            KeyboardRow(zones, topRow, pressedId, shiftLatched, 48.dp, 13.sp, false)
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                for (char in homeRow) {
                    val id = charId(char)
                    MmKey(
                        zones = zones,
                        id = id,
                        label = displayChar(char, shiftLatched),
                        pressed = pressedId == id,
                        modifier = Modifier.weight(1f),
                    )
                }
                MmKey(
                    zones = zones,
                    id = ID_ENTER,
                    label = "ENT",
                    pressed = pressedId == ID_ENTER,
                    modifier = Modifier.weight(1.7f),
                    labelSize = 10.sp,
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MmKey(
                    zones = zones,
                    id = ID_LATCH_SHIFT,
                    label = "SHF",
                    pressed = pressedId == ID_LATCH_SHIFT,
                    modifier = Modifier.weight(1.5f),
                    labelSize = 10.sp,
                    latched = shiftLatched,
                )
                for (char in bottomRow) {
                    val id = charId(char)
                    MmKey(
                        zones = zones,
                        id = id,
                        label = displayChar(char, shiftLatched),
                        pressed = pressedId == id,
                        modifier = Modifier.weight(1f),
                    )
                }
                MmKey(
                    zones = zones,
                    id = ID_DEL,
                    label = "DEL",
                    pressed = pressedId == ID_DEL,
                    modifier = Modifier.weight(1.2f),
                    labelSize = 10.sp,
                )
            }
            PortraitControlRow(zones, pressedId, ctrlLatched, altLatched, symLatched)
            Text(
                text = panelStatus(connection, ctrlLatched, altLatched, shiftLatched, symLatched),
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
fun LandscapeKeyboardPanel(
    viewModel: TouchpadViewModel,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val ctrlLatched by viewModel.ctrlLatched.collectAsState()
    val altLatched by viewModel.altLatched.collectAsState()
    val shiftLatched by viewModel.shiftLatched.collectAsState()
    val symLatched by viewModel.symLatched.collectAsState()

    val zones = remember { KeyZoneMap() }
    var pressedId by remember { mutableStateOf<String?>(null) }
    val currentToggle by rememberUpdatedState(onToggle)

    val host = remember(viewModel) {
        object : UnifiedKeyHost {
            override fun isHoldable(id: String): Boolean = id.startsWith("mouse_")

            override fun isRepeatable(id: String): Boolean =
                id.startsWith("char_") ||
                    id.startsWith("digit_") ||
                    id == ID_SPACE ||
                    id == ID_BKSP ||
                    id == ID_DEL ||
                    id == ID_ENTER ||
                    id == ID_UP ||
                    id == ID_DOWN ||
                    id == ID_LEFT ||
                    id == ID_RIGHT

            override fun onTap(id: String) {
                if (id == UI_TOGGLE_ID) currentToggle() else fire(id, viewModel)
            }

            override fun onHoldStart(id: String) {
                when (id) {
                    ID_MOUSE_LEFT -> viewModel.pressButton(MouseButton.LEFT, true)
                    ID_MOUSE_RIGHT -> viewModel.pressButton(MouseButton.RIGHT, true)
                    ID_MOUSE_MIDDLE -> viewModel.pressButton(MouseButton.MIDDLE, true)
                }
            }

            override fun onHoldEnd(id: String) {
                when (id) {
                    ID_MOUSE_LEFT -> viewModel.pressButton(MouseButton.LEFT, false)
                    ID_MOUSE_RIGHT -> viewModel.pressButton(MouseButton.RIGHT, false)
                    ID_MOUSE_MIDDLE -> viewModel.pressButton(MouseButton.MIDDLE, false)
                }
            }

            override fun onRepeat(id: String) = fire(id, viewModel)

            override fun onPressedChange(id: String?) {
                pressedId = id
            }
        }
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .background(MmPanel)
            .border(width = 2.dp, color = MmInk)
            .unifiedSurfaceRoot(zones)
            .unifiedInputGestures(zones, host, viewModel::send),
    ) {
        HalftoneDots(Modifier.matchParentSize())
        Column(
            Modifier
                .fillMaxSize()
                .padding(horizontal = 6.dp, vertical = 4.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MmKey(
                    zones = zones,
                    id = ID_MOUSE_LEFT,
                    label = "L",
                    pressed = pressedId == ID_MOUSE_LEFT,
                    modifier = Modifier.weight(1f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_MOUSE_RIGHT,
                    label = "R",
                    pressed = pressedId == ID_MOUSE_RIGHT,
                    modifier = Modifier.weight(1f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_MOUSE_MIDDLE,
                    label = "M",
                    pressed = pressedId == ID_MOUSE_MIDDLE,
                    modifier = Modifier.weight(1f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones, ID_ESC, "ESC", pressedId == ID_ESC, Modifier.weight(0.8f),
                    labelSize = 9.sp, fillHeight = true,
                )
                MmKey(
                    zones, ID_TAB, "TAB", pressedId == ID_TAB, Modifier.weight(0.8f),
                    labelSize = 9.sp, fillHeight = true,
                )
                MmKey(
                    zones, ID_HOME, "HM", pressedId == ID_HOME, Modifier.weight(0.8f),
                    labelSize = 9.sp, fillHeight = true,
                )
                MmKey(
                    zones, ID_END, "END", pressedId == ID_END, Modifier.weight(0.8f),
                    labelSize = 9.sp, fillHeight = true,
                )
                MmKey(
                    zones, ID_PGUP, "PG+", pressedId == ID_PGUP, Modifier.weight(0.8f),
                    labelSize = 9.sp, fillHeight = true,
                )
                MmKey(
                    zones, ID_PGDN, "PG-", pressedId == ID_PGDN, Modifier.weight(0.8f),
                    labelSize = 9.sp, fillHeight = true,
                )
            }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val shown = if (symLatched) symbolChars else digitChars
                for (index in shown.indices) {
                    val id = digitId(index)
                    MmKey(
                        zones = zones,
                        id = id,
                        label = shown[index].toString(),
                        pressed = pressedId == id,
                        modifier = Modifier.weight(1f),
                        labelSize = 12.sp,
                        fillHeight = true,
                    )
                }
                MmKey(
                    zones = zones,
                    id = ID_BKSP,
                    label = "BKSP",
                    pressed = pressedId == ID_BKSP,
                    modifier = Modifier.weight(1.4f),
                    labelSize = 9.sp,
                    fillHeight = true,
                )
            }
            KeyboardRow(
                zones, topRow, pressedId, shiftLatched, 0.dp, 12.sp, true,
                Modifier
                    .fillMaxWidth()
                    .weight(1f),
            )
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                for (char in homeRow) {
                    val id = charId(char)
                    MmKey(
                        zones = zones,
                        id = id,
                        label = displayChar(char, shiftLatched),
                        pressed = pressedId == id,
                        modifier = Modifier.weight(1f),
                        labelSize = 12.sp,
                        fillHeight = true,
                    )
                }
                MmKey(
                    zones = zones,
                    id = ID_ENTER,
                    label = "ENT",
                    pressed = pressedId == ID_ENTER,
                    modifier = Modifier.weight(1.3f),
                    labelSize = 9.sp,
                    fillHeight = true,
                )
            }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MmKey(
                    zones = zones,
                    id = ID_LATCH_SHIFT,
                    label = "SHF",
                    pressed = pressedId == ID_LATCH_SHIFT,
                    modifier = Modifier.weight(1.2f),
                    labelSize = 9.sp,
                    latched = shiftLatched,
                    fillHeight = true,
                )
                for (char in bottomRow) {
                    val id = charId(char)
                    MmKey(
                        zones = zones,
                        id = id,
                        label = displayChar(char, shiftLatched),
                        pressed = pressedId == id,
                        modifier = Modifier.weight(1f),
                        labelSize = 12.sp,
                        fillHeight = true,
                    )
                }
                MmKey(
                    zones = zones,
                    id = ID_DEL,
                    label = "DEL",
                    pressed = pressedId == ID_DEL,
                    modifier = Modifier.weight(1f),
                    labelSize = 9.sp,
                    fillHeight = true,
                )
            }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MmKey(
                    zones = zones,
                    id = ID_LATCH_CTRL,
                    label = "CTL",
                    pressed = pressedId == ID_LATCH_CTRL,
                    modifier = Modifier.weight(0.9f),
                    labelSize = 9.sp,
                    latched = ctrlLatched,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_LATCH_ALT,
                    label = "ALT",
                    pressed = pressedId == ID_LATCH_ALT,
                    modifier = Modifier.weight(0.9f),
                    labelSize = 9.sp,
                    latched = altLatched,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_LATCH_SYM,
                    label = "SYM",
                    pressed = pressedId == ID_LATCH_SYM,
                    modifier = Modifier.weight(1f),
                    labelSize = 9.sp,
                    latched = symLatched,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_COMMA,
                    label = ",",
                    pressed = pressedId == ID_COMMA,
                    modifier = Modifier.weight(0.8f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_SPACE,
                    label = "SPACE",
                    pressed = pressedId == ID_SPACE,
                    modifier = Modifier.weight(2.2f),
                    labelSize = 9.sp,
                    labelFamily = CourierPrimeFamily,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_DOT,
                    label = ".",
                    pressed = pressedId == ID_DOT,
                    modifier = Modifier.weight(0.8f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_LEFT,
                    label = "←",
                    pressed = pressedId == ID_LEFT,
                    modifier = Modifier.weight(0.8f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_UP,
                    label = "↑",
                    pressed = pressedId == ID_UP,
                    modifier = Modifier.weight(0.8f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_DOWN,
                    label = "↓",
                    pressed = pressedId == ID_DOWN,
                    modifier = Modifier.weight(0.8f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
                MmKey(
                    zones = zones,
                    id = ID_RIGHT,
                    label = "→",
                    pressed = pressedId == ID_RIGHT,
                    modifier = Modifier.weight(0.8f),
                    labelSize = 12.sp,
                    fillHeight = true,
                )
            }
        }
        UiToggleChip(
            zones = zones,
            pressed = pressedId == UI_TOGGLE_ID,
            hide = true,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(2.dp),
        )
    }
}

@Composable
fun LandscapeHiddenPanel(
    viewModel: TouchpadViewModel,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val zones = remember { KeyZoneMap() }
    var pressedId by remember { mutableStateOf<String?>(null) }
    val currentToggle by rememberUpdatedState(onToggle)

    val host = remember(viewModel) {
        object : UnifiedKeyHost {
            override fun isHoldable(id: String): Boolean = false

            override fun isRepeatable(id: String): Boolean = false

            override fun onTap(id: String) {
                if (id == UI_TOGGLE_ID) currentToggle()
            }

            override fun onHoldStart(id: String) = Unit

            override fun onHoldEnd(id: String) = Unit

            override fun onRepeat(id: String) = Unit

            override fun onPressedChange(id: String?) {
                pressedId = id
            }
        }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MmPanel)
            .border(width = 2.dp, color = MmInk)
            .unifiedSurfaceRoot(zones)
            .unifiedInputGestures(zones, host, viewModel::send),
    ) {
        HalftoneDots(Modifier.matchParentSize())
        UiToggleChip(
            zones = zones,
            pressed = pressedId == UI_TOGGLE_ID,
            hide = false,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(6.dp),
        )
    }
}

@Composable
private fun BoxScope.UiToggleChip(
    zones: KeyZoneMap,
    pressed: Boolean,
    hide: Boolean,
    modifier: Modifier = Modifier,
) {
    val face = if (pressed) MmInk else MmPanel
    val ink = if (pressed) MmPanel else MmInk
    Box(
        modifier = modifier
            .keyZone(zones, UI_TOGGLE_ID)
            .size(width = 44.dp, height = 32.dp),
    ) {
        Box(
            Modifier
                .matchParentSize()
                .offset(x = 2.dp, y = 2.dp)
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
                text = if (hide) "X" else "+",
                style = TextStyle(
                    fontFamily = ArchivoBlackFamily,
                    fontSize = 11.sp,
                    color = ink,
                ),
            )
        }
    }
}

@Composable
private fun PortraitMouseRow(zones: KeyZoneMap, pressedId: String?) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        MmKey(
            zones = zones,
            id = ID_MOUSE_LEFT,
            label = "L",
            pressed = pressedId == ID_MOUSE_LEFT,
            modifier = Modifier.weight(1.3f),
        )
        MmKey(
            zones = zones,
            id = ID_MOUSE_RIGHT,
            label = "R",
            pressed = pressedId == ID_MOUSE_RIGHT,
            modifier = Modifier.weight(1.3f),
        )
        MmKey(
            zones = zones,
            id = ID_MOUSE_MIDDLE,
            label = "M",
            pressed = pressedId == ID_MOUSE_MIDDLE,
            modifier = Modifier.weight(1.3f),
        )
        MmKey(zones, ID_ESC, "ESC", pressedId == ID_ESC, Modifier.weight(1f), labelSize = 9.sp)
        MmKey(zones, ID_TAB, "TAB", pressedId == ID_TAB, Modifier.weight(1f), labelSize = 9.sp)
        MmKey(
            zones, ID_HOME, "HM", pressedId == ID_HOME, Modifier.weight(1f),
            labelSize = 9.sp,
        )
        MmKey(
            zones, ID_END, "END", pressedId == ID_END, Modifier.weight(1f),
            labelSize = 9.sp,
        )
        MmKey(
            zones, ID_PGUP, "PG+", pressedId == ID_PGUP, Modifier.weight(1f),
            labelSize = 9.sp,
        )
        MmKey(
            zones, ID_PGDN, "PG-", pressedId == ID_PGDN, Modifier.weight(1f),
            labelSize = 9.sp,
        )
    }
}

@Composable
private fun PortraitDigitRow(zones: KeyZoneMap, pressedId: String?, symLatched: Boolean) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val shown = if (symLatched) symbolChars else digitChars
        for (index in shown.indices) {
            val id = digitId(index)
            MmKey(
                zones = zones,
                id = id,
                label = shown[index].toString(),
                pressed = pressedId == id,
                modifier = Modifier.weight(1f),
                labelSize = 13.sp,
            )
        }
        MmKey(
            zones = zones,
            id = ID_BKSP,
            label = "BKSP",
            pressed = pressedId == ID_BKSP,
            modifier = Modifier.weight(1.6f),
            labelSize = 10.sp,
        )
    }
}

@Composable
private fun PortraitControlRow(
    zones: KeyZoneMap,
    pressedId: String?,
    ctrlLatched: Boolean,
    altLatched: Boolean,
    symLatched: Boolean,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        MmKey(
            zones = zones,
            id = ID_LATCH_CTRL,
            label = "CTL",
            pressed = pressedId == ID_LATCH_CTRL,
            modifier = Modifier.weight(1f),
            labelSize = 10.sp,
            latched = ctrlLatched,
        )
        MmKey(
            zones = zones,
            id = ID_LATCH_ALT,
            label = "ALT",
            pressed = pressedId == ID_LATCH_ALT,
            modifier = Modifier.weight(1f),
            labelSize = 10.sp,
            latched = altLatched,
        )
        MmKey(
            zones = zones,
            id = ID_LATCH_SYM,
            label = "SYM",
            pressed = pressedId == ID_LATCH_SYM,
            modifier = Modifier.weight(1.2f),
            labelSize = 10.sp,
            latched = symLatched,
        )
        MmKey(
            zones = zones,
            id = ID_COMMA,
            label = ",",
            pressed = pressedId == ID_COMMA,
            modifier = Modifier.weight(1f),
        )
        MmKey(
            zones = zones,
            id = ID_SPACE,
            label = "SPACE",
            pressed = pressedId == ID_SPACE,
            modifier = Modifier.weight(3.2f),
            labelSize = 10.sp,
            labelFamily = CourierPrimeFamily,
        )
        MmKey(
            zones = zones,
            id = ID_DOT,
            label = ".",
            pressed = pressedId == ID_DOT,
            modifier = Modifier.weight(1f),
        )
        MmKey(
            zones = zones,
            id = ID_LEFT,
            label = "←",
            pressed = pressedId == ID_LEFT,
            modifier = Modifier.weight(1f),
        )
        MmKey(
            zones = zones,
            id = ID_UP,
            label = "↑",
            pressed = pressedId == ID_UP,
            modifier = Modifier.weight(1f),
        )
        MmKey(
            zones = zones,
            id = ID_DOWN,
            label = "↓",
            pressed = pressedId == ID_DOWN,
            modifier = Modifier.weight(1f),
        )
        MmKey(
            zones = zones,
            id = ID_RIGHT,
            label = "→",
            pressed = pressedId == ID_RIGHT,
            modifier = Modifier.weight(1f),
        )
    }
}

private fun fire(id: String, viewModel: TouchpadViewModel) {
    when (id) {
        ID_MOUSE_LEFT -> viewModel.clickButton(MouseButton.LEFT)
        ID_MOUSE_RIGHT -> viewModel.clickButton(MouseButton.RIGHT)
        ID_MOUSE_MIDDLE -> viewModel.clickButton(MouseButton.MIDDLE)
        ID_LATCH_CTRL -> viewModel.toggleCtrl()
        ID_LATCH_ALT -> viewModel.toggleAlt()
        ID_LATCH_SHIFT -> viewModel.toggleShift()
        ID_LATCH_SYM -> viewModel.toggleSym()
        ID_ESC -> viewModel.tapSpecial(Evdev.KEY_ESC)
        ID_TAB -> viewModel.tapSpecial(Evdev.KEY_TAB)
        ID_HOME -> viewModel.tapSpecial(Evdev.KEY_HOME)
        ID_END -> viewModel.tapSpecial(Evdev.KEY_END)
        ID_PGUP -> viewModel.tapSpecial(Evdev.KEY_PAGEUP)
        ID_PGDN -> viewModel.tapSpecial(Evdev.KEY_PAGEDOWN)
        ID_ENTER -> viewModel.tapSpecial(Evdev.KEY_ENTER)
        ID_BKSP -> viewModel.tapSpecial(Evdev.KEY_BACKSPACE)
        ID_DEL -> viewModel.tapSpecial(Evdev.KEY_DELETE)
        ID_UP -> viewModel.tapSpecial(Evdev.KEY_UP)
        ID_DOWN -> viewModel.tapSpecial(Evdev.KEY_DOWN)
        ID_LEFT -> viewModel.tapSpecial(Evdev.KEY_LEFT)
        ID_RIGHT -> viewModel.tapSpecial(Evdev.KEY_RIGHT)
        ID_SPACE -> viewModel.typeChar(' ')
        ID_COMMA -> viewModel.typeChar(',')
        ID_DOT -> viewModel.typeChar('.')
        else -> {
            val digit = digitIndex(id)
            if (digit != null) {
                val useSym = viewModel.symLatched.value
                viewModel.typeChar(if (useSym) symbolChars[digit] else digitChars[digit])
                return
            }
            val char = charOf(id)
            if (char != null) viewModel.typeChar(char)
        }
    }
}

private fun digitIndex(id: String): Int? =
    if (id.startsWith("digit_")) id.removePrefix("digit_").toIntOrNull() else null

private fun charOf(id: String): Char? =
    if (id.startsWith("char_") && id.length == 6) id[5] else null

private fun displayChar(char: Char, shift: Boolean): String =
    if (shift) char.uppercaseChar().toString() else char.toString()

@Composable
private fun KeyboardRow(
    zones: KeyZoneMap,
    keys: List<Char>,
    pressedId: String?,
    shift: Boolean = false,
    minHeight: androidx.compose.ui.unit.Dp = 48.dp,
    labelSize: TextUnit = 13.sp,
    fillHeight: Boolean = false,
    modifier: Modifier = Modifier.fillMaxWidth(),
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(if (fillHeight) 4.dp else 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (char in keys) {
            val id = charId(char)
            MmKey(
                zones = zones,
                id = id,
                label = displayChar(char, shift),
                pressed = pressedId == id,
                modifier = Modifier.weight(1f),
                minHeight = minHeight,
                labelSize = labelSize,
                fillHeight = fillHeight,
            )
        }
    }
}

@Composable
private fun RowScope.MmKey(
    zones: KeyZoneMap,
    id: String,
    label: String,
    pressed: Boolean,
    modifier: Modifier = Modifier,
    minHeight: androidx.compose.ui.unit.Dp = 48.dp,
    labelSize: TextUnit = 13.sp,
    labelFamily: FontFamily = ArchivoBlackFamily,
    latched: Boolean = false,
    fillHeight: Boolean = false,
) {
    val active = pressed || latched
    val face = if (active) MmInk else MmRaised
    val ink = if (active) MmPanel else MmInk
    val outer = if (fillHeight) {
        modifier
            .keyZone(zones, id)
            .skewX(SKEW_DEGREES)
            .fillMaxHeight()
    } else {
        modifier
            .keyZone(zones, id)
            .skewX(SKEW_DEGREES)
            .heightIn(min = minHeight)
    }
    Box(
        modifier = outer,
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
internal fun HalftoneDots(modifier: Modifier = Modifier) {
    Canvas(modifier) {
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
}

@Composable
internal fun MmDoubleRule() {
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

internal fun padHint(
    connection: ConnectionState,
    ctrl: Boolean,
    alt: Boolean,
    shift: Boolean,
    sym: Boolean,
): String {
    if (connection != ConnectionState.CONNECTED) return "BAGLANTI YOK · TUSLARA BASMAK YOK"
    val latches = buildString {
        if (ctrl) append(" CTL")
        if (alt) append(" ALT")
        if (shift) append(" SHF")
        if (sym) append(" SYM")
    }
    return "HAZIR · SURUKLE FAREYI SÜRER · 2 PARMAK KAYDIRIR$latches"
}

internal fun panelStatus(
    connection: ConnectionState,
    ctrl: Boolean,
    alt: Boolean,
    shift: Boolean,
    sym: Boolean,
): String {
    val base = when (connection) {
        ConnectionState.CONNECTED -> "READY"
        ConnectionState.CONNECTING -> "CONNECTING"
        ConnectionState.RECONNECTING -> "RECONNECTING"
        ConnectionState.DISCONNECTED -> "NO LINK"
    }
    val latches = buildString {
        if (ctrl) append(" · CTRL")
        if (alt) append(" · ALT")
        if (shift) append(" · SHIFT")
        if (sym) append(" · SYM")
    }
    return base + latches
}
