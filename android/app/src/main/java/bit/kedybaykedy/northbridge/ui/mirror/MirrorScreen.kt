package bit.kedybaykedy.northbridge.ui.mirror

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
import androidx.compose.runtime.saveable.rememberSaveable
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
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.input.NativePointer
import bit.kedybaykedy.northbridge.input.PointerMode
import bit.kedybaykedy.northbridge.ui.theme.AntonFamily
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import bit.kedybaykedy.northbridge.ui.theme.MmBlack
import bit.kedybaykedy.northbridge.ui.theme.MmGhost
import bit.kedybaykedy.northbridge.ui.theme.MmInk
import bit.kedybaykedy.northbridge.ui.theme.MmKicker
import bit.kedybaykedy.northbridge.ui.theme.MmMeta
import bit.kedybaykedy.northbridge.ui.theme.MmPanel
import bit.kedybaykedy.northbridge.ui.theme.MmSecondary
import bit.kedybaykedy.northbridge.ui.theme.MmShadow
import kotlin.math.tan

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

@Composable
private fun DoubleRule(modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth()) {
        Box(Modifier.fillMaxWidth().height(3.dp).background(MmInk))
        Spacer(Modifier.height(2.dp))
        Box(Modifier.fillMaxWidth().height(1.dp).background(MmInk))
    }
}

@Composable
private fun HalftoneBand(modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val step = 14.dp.toPx()
        val radius = 2.6.dp.toPx()
        val dot = MmInk.copy(alpha = 0.85f)
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
    val edge = if (enabled) MmInk else MmMeta
    val textColor = when {
        primary && enabled -> MmPanel
        primary -> MmSecondary
        enabled -> MmInk
        else -> MmMeta
    }
    Box(
        modifier = modifier
            .height(48.dp)
            .clickable(enabled = enabled, onClick = onClick)
            .drawBehind {
                val slant = size.height * 0.1763f
                val shift = 5.dp.toPx()
                val shadow = Path().apply {
                    moveTo(slant + shift, shift)
                    lineTo(size.width + slant + shift, shift)
                    lineTo(size.width - slant + shift, size.height + shift)
                    lineTo(-slant + shift, size.height + shift)
                    close()
                }
                drawPath(path = shadow, color = MmShadow)
                val body = Path().apply {
                    moveTo(slant, 0f)
                    lineTo(size.width + slant, 0f)
                    lineTo(size.width - slant, size.height)
                    lineTo(-slant, size.height)
                    close()
                }
                if (primary) {
                    drawPath(path = body, color = if (enabled) MmInk else MmGhost)
                } else {
                    drawPath(path = body, color = MmBlack)
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
private fun Masthead(alias: String, stateLabel: String) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            text = "AYNA \u00B7 " + (if (alias.isEmpty()) "CIHAZ YOK" else alias.uppercase()) + " \u00B7 " + stateLabel,
            style = TextStyle(
                fontFamily = ArchivoBlackFamily,
                fontSize = 11.sp,
                letterSpacing = 2.sp,
                color = MmKicker,
            ),
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        )
        Text(
            text = "AYNA",
            style = TextStyle(
                fontFamily = AntonFamily,
                fontStyle = FontStyle.Italic,
                fontSize = 26.sp,
                letterSpacing = 1.sp,
                color = MmInk,
                shadow = Shadow(color = MmShadow, offset = Offset(3f, 3f)),
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
private fun PointerToggle() {
    var selection by rememberSaveable { mutableStateOf(NativePointer.mode.value.name) }
    val status by NativePointer.status.collectAsState()
    val options = listOf(
        PointerMode.AUTO to "OTOMATIK",
        PointerMode.REAL to "GERCEK",
        PointerMode.VIRTUAL to "SANAL",
    )
    Column(Modifier.fillMaxWidth()) {
        Text(
            text = "IMLEC",
            style = TextStyle(
                fontFamily = ArchivoBlackFamily,
                fontSize = 11.sp,
                letterSpacing = 2.sp,
                color = MmKicker,
            ),
        )
        Spacer(Modifier.height(8.dp))
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            for ((mode, label) in options) {
                val selected = selection == mode.name
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .background(if (selected) MmInk else MmBlack)
                        .clickable {
                            selection = mode.name
                            NativePointer.setMode(mode)
                        }
                        .padding(horizontal = 8.dp, vertical = 12.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = label,
                        style = TextStyle(
                            fontFamily = ArchivoBlackFamily,
                            fontSize = 11.sp,
                            letterSpacing = 1.sp,
                            color = if (selected) MmPanel else MmSecondary,
                        ),
                    )
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(
            text = status,
            style = TextStyle(
                fontFamily = CourierPrimeFamily,
                fontSize = 12.sp,
                color = MmSecondary,
            ),
        )
    }
}

@Composable
fun MirrorScreen(viewModel: MirrorViewModel = viewModel()) {
    val uiState by viewModel.uiState.collectAsState()
    val meta by viewModel.meta.collectAsState()
    val alias by viewModel.deviceAlias.collectAsState()
    val failure by viewModel.error.collectAsState()

    val stateLabel = when (uiState) {
        MirrorUiState.MIRRORING -> "YAYINDA"
        MirrorUiState.ERROR -> "HATA"
        MirrorUiState.IDLE -> "KAPALI"
    }
    val statusLine = when (uiState) {
        MirrorUiState.MIRRORING -> "TELEFON EKRANI AKTIF"
        MirrorUiState.ERROR -> "YAYIN DURDU"
        MirrorUiState.IDLE -> "YAYIN KAPALI"
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(MmBlack)
            .padding(16.dp),
    ) {
        Masthead(alias = alias, stateLabel = stateLabel)
        Spacer(Modifier.height(12.dp))
        HalftoneBand(Modifier.fillMaxWidth().height(64.dp))
        Spacer(Modifier.height(12.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .background(MmPanel)
                .padding(horizontal = 20.dp, vertical = 18.dp),
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(
                    text = statusLine,
                    style = TextStyle(
                        fontFamily = ArchivoBlackFamily,
                        fontSize = 11.sp,
                        letterSpacing = 2.sp,
                        color = MmKicker,
                    ),
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    text = if (meta.isEmpty()) "\u2014" else meta,
                    style = TextStyle(
                        fontFamily = CourierPrimeFamily,
                        fontSize = 12.sp,
                        letterSpacing = 1.sp,
                        color = MmMeta,
                    ),
                )
            }
        }
        val currentFailure = failure
        if (currentFailure != null) {
            Spacer(Modifier.height(8.dp))
            Text(
                text = currentFailure,
                style = TextStyle(
                    fontFamily = CourierPrimeFamily,
                    fontSize = 12.sp,
                    color = MmInk,
                ),
            )
        }
        Spacer(Modifier.height(12.dp))
        PointerToggle()
        Spacer(Modifier.height(12.dp))
        Text(
            text = "Masaustundeki AYNA penceresi acilacak \u2014 bilgisayarda etkilesim pencereye tiklayarak geri gelir.",
            style = TextStyle(
                fontFamily = CourierPrimeFamily,
                fontSize = 12.sp,
                color = MmSecondary,
            ),
        )
        Spacer(Modifier.weight(1f))
        Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            SkewButton(
                label = "BASLAT",
                primary = true,
                enabled = uiState != MirrorUiState.MIRRORING,
                onClick = viewModel::start,
                modifier = Modifier.weight(1f),
            )
            SkewButton(
                label = "DURDUR",
                primary = false,
                enabled = uiState == MirrorUiState.MIRRORING,
                onClick = viewModel::stop,
                modifier = Modifier.weight(1f),
            )
        }
    }
}
