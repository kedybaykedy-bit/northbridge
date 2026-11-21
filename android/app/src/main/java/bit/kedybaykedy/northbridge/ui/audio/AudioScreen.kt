package bit.kedybaykedy.northbridge.ui.audio

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.ui.theme.AntonFamily
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import kotlin.math.tan

private val Black = Color(0xFF000000)
private val Panel = Color(0xFF0A0A0A)
private val Ghost = Color(0xFF262626)
private val HardShadow = Color(0xFF4A4A4A)
private val Gray6 = Color(0xFF8A8A8A)
private val Gray7 = Color(0xFF9A9A9A)
private val Gray8 = Color(0xFFC0C0C0)
private val White = Color(0xFFF2F2F2)

private fun Modifier.skewX(degrees: Float): Modifier = drawWithContent {
    val tangent = tan(degrees * 0.017453292f)
    drawIntoCanvas { canvas ->
        canvas.save()
        canvas.translate(-tangent * size.height / 2f, 0f)
        canvas.skew(tangent, 0f)
        drawContent()
        canvas.restore()
    }
}

private fun Modifier.miniBox(): Modifier = drawBehind {
    val shift = 6.dp.toPx()
    drawPath(
        path = Path().apply {
            moveTo(shift, shift)
            lineTo(size.width + shift, shift)
            lineTo(size.width + shift, size.height + shift)
            lineTo(shift, size.height + shift)
            close()
        },
        color = HardShadow,
    )
    drawRect(color = Panel, topLeft = Offset.Zero, size = size)
    drawRect(color = White, topLeft = Offset.Zero, size = size, style = Stroke(width = 2.dp.toPx()))
}

@Composable
private fun DoubleRule(modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth()) {
        Box(Modifier.fillMaxWidth().height(3.dp).background(White))
        Spacer(Modifier.height(2.dp))
        Box(Modifier.fillMaxWidth().height(1.dp).background(White))
    }
}

@Composable
private fun HalftoneBand(modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val step = 14.dp.toPx()
        val radius = 2.6.dp.toPx()
        val dot = White.copy(alpha = 0.85f)
        var row = 0
        var y = 0f
        while (y <= size.height) {
            var x = if (row % 2 == 1) step / 2f else 0f
            while (x <= size.width) {
                drawCircle(color = dot, radius = radius, center = Offset(x, y))
                x += step
            }
            y += step
            row++
        }
    }
}

@Composable
private fun SkewButton(
    label: String,
    primary: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val edge = if (enabled) White else Gray6
    val textColor = when {
        primary && enabled -> Panel
        primary -> Gray8
        enabled -> White
        else -> Gray6
    }
    Box(
        modifier = modifier
            .height(48.dp)
            .clickable(enabled = enabled, onClick = onClick)
            .drawBehind {
                val slant = size.height * 0.1763f
                val shadowShift = 5.dp.toPx()
                val shadow = Path().apply {
                    moveTo(slant + shadowShift, shadowShift)
                    lineTo(size.width + slant + shadowShift, shadowShift)
                    lineTo(size.width - slant + shadowShift, size.height + shadowShift)
                    lineTo(-slant + shadowShift, size.height + shadowShift)
                    close()
                }
                drawPath(path = shadow, color = HardShadow)
                val body = Path().apply {
                    moveTo(slant, 0f)
                    lineTo(size.width + slant, 0f)
                    lineTo(size.width - slant, size.height)
                    lineTo(-slant, size.height)
                    close()
                }
                if (primary) {
                    drawPath(path = body, color = if (enabled) White else Ghost)
                } else {
                    drawPath(path = body, color = Black)
                    drawPath(path = body, color = edge, style = Stroke(width = 2.dp.toPx()))
                }
            }
            .padding(horizontal = 26.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = label,
            style = TextStyle(
                fontFamily = AntonFamily,
                fontSize = 16.sp,
                letterSpacing = 1.sp,
                color = textColor,
            ),
        )
    }
}

@Composable
private fun DirectionStat(kicker: String, value: String, modifier: Modifier = Modifier) {
    Column(modifier) {
        Text(
            text = kicker,
            style = TextStyle(
                fontFamily = ArchivoBlackFamily,
                fontSize = 10.sp,
                letterSpacing = 2.sp,
                color = Gray7,
            ),
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text = value,
            style = TextStyle(
                fontFamily = AntonFamily,
                fontSize = 24.sp,
                color = White,
                shadow = Shadow(color = HardShadow, offset = Offset(2f, 2f)),
            ),
            modifier = Modifier.skewX(-6f),
        )
    }
}

@Composable
private fun Masthead(alias: String, stateLabel: String) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            text = (if (alias.isEmpty()) "MASAÜSTÜ" else alias) + " · " + stateLabel,
            style = TextStyle(
                fontFamily = ArchivoBlackFamily,
                fontSize = 11.sp,
                letterSpacing = 2.sp,
                color = Gray7,
            ),
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        )
        Text(
            text = "SES KÖPRÜSÜ",
            style = TextStyle(
                fontFamily = AntonFamily,
                fontSize = 26.sp,
                letterSpacing = 1.sp,
                color = White,
                shadow = Shadow(color = HardShadow, offset = Offset(3f, 3f)),
            ),
            modifier = Modifier
                .padding(horizontal = 16.dp)
                .padding(bottom = 10.dp)
                .skewX(-6f),
        )
        DoubleRule()
    }
}

@Composable
fun AudioScreen(viewModel: AudioViewModel = viewModel()) {
    val uiState by viewModel.uiState.collectAsState()
    val meta by viewModel.meta.collectAsState()
    val alias by viewModel.deviceAlias.collectAsState()
    val codecLabel by viewModel.codecLabel.collectAsState()
    val source by viewModel.source.collectAsState()
    val micMuted by viewModel.micMuted.collectAsState()
    val speakerMuted by viewModel.speakerMuted.collectAsState()
    val sent by viewModel.sent.collectAsState()
    val received by viewModel.received.collectAsState()
    val error by viewModel.error.collectAsState()

    val stateLabel = when (uiState) {
        AudioUiState.ACTIVE -> "CANLI"
        AudioUiState.OPENING -> "AÇILIYOR"
        AudioUiState.CLOSED -> "KAPANDI"
        AudioUiState.ERROR -> "HATA"
        AudioUiState.IDLE -> "KAPALI"
    }
    val running = uiState == AudioUiState.ACTIVE || uiState == AudioUiState.OPENING

    val context = LocalContext.current
    var askedOnce by remember { mutableStateOf(false) }
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        askedOnce = true
        if (granted) {
            viewModel.start()
        }
    }
    fun onStartClicked() {
        val granted = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED
        if (granted) {
            viewModel.start()
        } else {
            permissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(Black)
            .padding(16.dp),
    ) {
        Masthead(alias = alias, stateLabel = stateLabel)
        Spacer(Modifier.height(12.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .miniBox()
                .padding(horizontal = 16.dp, vertical = 14.dp),
        ) {
            Column {
                Text(
                    text = "YÖN · " + codecLabel,
                    style = TextStyle(
                        fontFamily = ArchivoBlackFamily,
                        fontSize = 10.sp,
                        letterSpacing = 2.sp,
                        color = Gray7,
                    ),
                )
                Spacer(Modifier.height(8.dp))
                Row(
                    horizontalArrangement = Arrangement.spacedBy(16.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    DirectionStat(
                        kicker = "TEL → PC",
                        value = sent.toString(),
                        modifier = Modifier.weight(1f),
                    )
                    DirectionStat(
                        kicker = "PC → TEL",
                        value = received.toString(),
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        HalftoneBand(Modifier.fillMaxWidth().height(24.dp))
        Spacer(Modifier.height(10.dp))
        Text(
            text = if (meta.isEmpty()) "—" else meta,
            style = TextStyle(
                fontFamily = CourierPrimeFamily,
                fontSize = 12.sp,
                letterSpacing = 1.sp,
                color = Gray6,
            ),
        )
        val failure = error
        if (failure != null) {
            Spacer(Modifier.height(4.dp))
            Text(
                text = failure,
                style = TextStyle(
                    fontFamily = CourierPrimeFamily,
                    fontSize = 12.sp,
                    color = White,
                ),
            )
        } else if (askedOnce && uiState == AudioUiState.IDLE) {
            Spacer(Modifier.height(4.dp))
            Text(
                text = "MİKROFON İZNİ GEREKLİ",
                style = TextStyle(
                    fontFamily = CourierPrimeFamily,
                    fontSize = 12.sp,
                    color = White,
                ),
            )
        }
        Spacer(Modifier.height(10.dp))
        Text(
            text = "KAYNAK",
            style = TextStyle(
                fontFamily = ArchivoBlackFamily,
                fontSize = 10.sp,
                letterSpacing = 2.sp,
                color = Gray7,
            ),
        )
        Spacer(Modifier.height(8.dp))
        Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            SkewButton(
                label = "MİKROFON",
                primary = source == AudioSourceOption.MIC,
                enabled = !running,
                onClick = { viewModel.setSource(AudioSourceOption.MIC) },
                modifier = Modifier.weight(1f),
            )
            SkewButton(
                label = "SİSTEM (ROOT)",
                primary = source == AudioSourceOption.SYSTEM,
                enabled = !running,
                onClick = { viewModel.setSource(AudioSourceOption.SYSTEM) },
                modifier = Modifier.weight(1f),
            )
        }
        Spacer(Modifier.height(4.dp))
        Text(
            text = "SİSTEM YALNIZCA MAGISK YETKİLİ BAĞLAMDA ÇALIŞIR, YOKSA HATA VERİR",
            style = TextStyle(
                fontFamily = CourierPrimeFamily,
                fontSize = 11.sp,
                color = Gray6,
            ),
        )
        Spacer(Modifier.height(10.dp))
        Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            SkewButton(
                label = if (micMuted) "MİK: KAPALI" else "MİK: AÇIK",
                primary = false,
                enabled = true,
                onClick = viewModel::toggleMicMute,
                modifier = Modifier.weight(1f),
            )
            SkewButton(
                label = if (speakerMuted) "PC SESİ: KAPALI" else "PC SESİ: AÇIK",
                primary = false,
                enabled = true,
                onClick = viewModel::toggleSpeakerMute,
                modifier = Modifier.weight(1f),
            )
        }
        Spacer(Modifier.height(10.dp))
        Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            SkewButton(
                label = "BAŞLAT",
                primary = true,
                enabled = uiState == AudioUiState.IDLE ||
                    uiState == AudioUiState.CLOSED ||
                    uiState == AudioUiState.ERROR,
                onClick = { onStartClicked() },
                modifier = Modifier.weight(1f),
            )
            SkewButton(
                label = "DURDUR",
                primary = false,
                enabled = running,
                onClick = viewModel::stop,
                modifier = Modifier.weight(1f),
            )
        }
    }
}
