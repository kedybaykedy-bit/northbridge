package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.kvm.drawDesktopCursor
import bit.kedybaykedy.northbridge.transport.ConnectionState
import kotlin.math.PI
import kotlin.math.tan

private val K0 = Color(0xFF000000)
private val K1 = Color(0xFF0A0A0A)
private val K2 = Color(0xFF1A1A1A)
private val G4 = Color(0xFF4A4A4A)
private val G5 = Color(0xFF6A6A6A)
private val G6 = Color(0xFF8A8A8A)
private val G7 = Color(0xFF9A9A9A)
private val G8 = Color(0xFFC0C0C0)
private val G9 = Color(0xFFCFCFCF)
private val Ink = Color(0xFFF2F2F2)

private fun Modifier.skewX(degrees: Float): Modifier = this.drawWithContent {
    val shear = tan(degrees * PI / 180.0).toFloat()
    drawIntoCanvas { canvas ->
        canvas.save()
        canvas.skew(shear, 0f)
        canvas.translate(-shear * size.height / 2f, 0f)
    }
    drawContent()
    drawIntoCanvas { canvas ->
        canvas.restore()
    }
}

@Composable
fun KvmScreen(viewModel: KvmViewModel = viewModel()) {
    val cursor by viewModel.cursor.collectAsState()
    val phase by viewModel.phase.collectAsState()
    val connection by AppGraph.inputChannel.state.collectAsState()
    val devices by AppGraph.deviceStore.devices.collectAsState()
    val activeFingerprint by AppGraph.deviceStore.activeFingerprint.collectAsState()
    val alias = devices.find { it.fingerprint == activeFingerprint }?.alias

    LaunchedEffect(Unit) {
        viewModel.requestCapture()
    }
    DisposableEffect(Unit) {
        onDispose {
            viewModel.releaseOnLeave()
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(K0),
    ) {
        Masthead(alias = alias, phase = phase, connection = connection)
        DoubleRule()
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(12.dp)
                .border(3.dp, Ink)
                .padding(4.dp)
                .border(1.dp, Ink),
        ) {
            Column(
                Modifier
                    .fillMaxSize()
                    .background(K0),
            ) {
                HalftoneBand(
                    Modifier
                        .fillMaxWidth()
                        .height(64.dp),
                )
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .background(K0)
                        .pointerInput(Unit) {
                            detectTapGestures { offset ->
                                val width = size.width
                                val height = size.height
                                if (width > 0 && height > 0) {
                                    viewModel.clickAt(
                                        offset.x.toDouble() / width,
                                        offset.y.toDouble() / height,
                                    )
                                }
                            }
                        },
                ) {
                    Canvas(Modifier.fillMaxSize()) {
                        val position = cursor
                        if (phase == KvmPhase.ACTIVE && position != null) {
                            drawDesktopCursor(
                                Offset(position.x * size.width, position.y * size.height),
                            )
                        }
                    }
                    if (phase != KvmPhase.ACTIVE || cursor == null) {
                        EmptyOverlay(phase = phase, connection = connection)
                    }
                }
            }
        }
        BottomBar(
            phase = phase,
            cursorX = cursor?.x,
            cursorY = cursor?.y,
            onToggle = {
                if (phase == KvmPhase.RELEASED) {
                    viewModel.requestCapture()
                } else {
                    viewModel.releaseCapture()
                }
            },
        )
    }
}

@Composable
private fun Masthead(alias: String?, phase: KvmPhase, connection: ConnectionState) {
    val phaseLabel = when (phase) {
        KvmPhase.WAITING -> "BEKLENIYOR"
        KvmPhase.ACTIVE -> "AKTIF"
        KvmPhase.RELEASED -> "BIRAKILDI"
    }
    val connectionLabel = when (connection) {
        ConnectionState.CONNECTED -> "BAGLI"
        ConnectionState.CONNECTING -> "BAGLANIYOR"
        ConnectionState.RECONNECTING -> "YENIDEN BAGLANIYOR"
        ConnectionState.DISCONNECTED -> "BAGLANTI YOK"
    }
    val deviceLabel = alias?.uppercase() ?: "CIHAZ YOK"
    Column(
        Modifier
            .fillMaxWidth()
            .background(K0)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .size(12.dp)
                    .background(if (connection == ConnectionState.CONNECTED) Ink else G5),
            )
            Spacer(Modifier.width(10.dp))
            Text(
                text = "KVM · $deviceLabel · $connectionLabel · $phaseLabel",
                style = TextStyle(
                    fontFamily = FontFamily.Default,
                    fontWeight = FontWeight.Black,
                    fontSize = 11.sp,
                    letterSpacing = 3.sp,
                    color = G7,
                ),
            )
        }
        Spacer(Modifier.height(10.dp))
        Text(
            text = "SECOND MONITOR",
            modifier = Modifier.skewX(-6f),
            style = TextStyle(
                fontFamily = FontFamily.Default,
                fontWeight = FontWeight.Black,
                fontStyle = FontStyle.Italic,
                fontSize = 28.sp,
                letterSpacing = 0.5.sp,
                color = Ink,
                shadow = Shadow(color = G4, offset = Offset(4f, 4f)),
            ),
        )
    }
}

@Composable
private fun DoubleRule() {
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(3.dp)
                .background(Ink),
        )
        Spacer(
            Modifier
                .fillMaxWidth()
                .height(2.dp),
        )
        Box(
            Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(Ink),
        )
    }
}

@Composable
private fun HalftoneBand(modifier: Modifier = Modifier) {
    Canvas(modifier.background(K0)) {
        val step = 14f
        val stagger = 7f
        val radius = 2.6f
        var row = 0
        var y = 0f
        while (y < size.height) {
            val xOffset = if (row % 2 == 1) stagger else 0f
            var x = xOffset
            while (x < size.width) {
                drawCircle(color = Ink, radius = radius, center = Offset(x, y), alpha = 0.85f)
                x += step
            }
            y += step
            row += 1
        }
    }
}

@Composable
private fun EmptyOverlay(phase: KvmPhase, connection: ConnectionState) {
    val kicker = when (phase) {
        KvmPhase.RELEASED -> "BIRAKILDI"
        else -> "BEKLENIYOR"
    }
    val hint = when {
        connection != ConnectionState.CONNECTED -> "Once Devices sekmesinde eslestirin ve baglanin."
        phase == KvmPhase.RELEASED -> "Yeniden yakalamak icin YAKALA dugmesine dokunun."
        else -> "Yakalama istendi. Fiziksel fareyi oynatin."
    }
    Column(
        Modifier
            .fillMaxSize()
            .background(K1)
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            text = kicker,
            style = TextStyle(
                fontFamily = FontFamily.Default,
                fontWeight = FontWeight.Black,
                fontSize = 11.sp,
                letterSpacing = 3.sp,
                color = G7,
            ),
        )
        Spacer(Modifier.height(12.dp))
        Text(
            text = hint,
            textAlign = TextAlign.Center,
            style = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
                lineHeight = 20.sp,
                color = G8,
            ),
        )
    }
}

@Composable
private fun BottomBar(
    phase: KvmPhase,
    cursorX: Float?,
    cursorY: Float?,
    onToggle: () -> Unit,
) {
    val coords = if (cursorX != null && cursorY != null) {
        "X %.4f Y %.4f".format(cursorX, cursorY)
    } else {
        "X ---- Y ----"
    }
    Column(
        Modifier
            .fillMaxWidth()
            .background(K0)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Text(
            text = coords,
            style = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                letterSpacing = 0.5.sp,
                color = G6,
            ),
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text = "SEK 1 · FIG 1 — IKINCI EKRAN",
            style = TextStyle(
                fontFamily = FontFamily.Default,
                fontWeight = FontWeight.Black,
                fontSize = 10.sp,
                letterSpacing = 2.sp,
                color = G7,
            ),
        )
        Spacer(Modifier.height(10.dp))
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MmButton(
                label = if (phase == KvmPhase.RELEASED) "YAKALA" else "BIRAK",
                primary = true,
                onClick = onToggle,
            )
            MmTag(label = if (phase == KvmPhase.ACTIVE) "AKTIF" else "PASIF")
        }
        Spacer(Modifier.height(4.dp))
        Text(
            text = "Dokunus: tik gonderir. Yakalamadayken masaustu fareyi surer.",
            style = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                color = G9,
            ),
        )
    }
}

@Composable
private fun MmTag(label: String) {
    Box(
        Modifier.defaultMinSize(minWidth = 96.dp, minHeight = 48.dp),
    ) {
        Box(
            Modifier
                .matchParentSize()
                .offset(5.dp, 5.dp)
                .background(G4),
        )
        Box(
            Modifier
                .skewX(-10f)
                .background(K2)
                .border(2.dp, Ink)
                .padding(horizontal = 26.dp, vertical = 14.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = label,
                modifier = Modifier.skewX(10f),
                style = TextStyle(
                    fontFamily = FontFamily.Default,
                    fontWeight = FontWeight.Black,
                    fontSize = 16.sp,
                    letterSpacing = 0.8.sp,
                    color = Ink,
                ),
            )
        }
    }
}

@Composable
private fun MmButton(label: String, primary: Boolean, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    var focused by remember { mutableStateOf(false) }
    val active = pressed || focused
    val background = when {
        primary && active -> K0
        primary -> Ink
        active -> Ink
        else -> K2
    }
    val foreground = when {
        primary && active -> Ink
        primary -> K0
        active -> K0
        else -> Ink
    }
    val shadowColor = if (active) Ink else G4
    val shadowOffset = if (active) 7.dp else 5.dp
    Box(
        Modifier.defaultMinSize(minWidth = 120.dp, minHeight = 48.dp),
    ) {
        Box(
            Modifier
                .matchParentSize()
                .offset(shadowOffset, shadowOffset)
                .background(shadowColor),
        )
        Box(
            Modifier
                .skewX(-10f)
                .background(background)
                .border(2.dp, Ink)
                .clickable(interactionSource = interaction, indication = null, onClick = onClick)
                .focusable()
                .onFocusChanged { focused = it.isFocused }
                .padding(horizontal = 26.dp, vertical = 14.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = label,
                modifier = Modifier.skewX(10f),
                style = TextStyle(
                    fontFamily = FontFamily.Default,
                    fontWeight = FontWeight.Black,
                    fontSize = 16.sp,
                    letterSpacing = 0.8.sp,
                    color = foreground,
                ),
            )
        }
    }
}
