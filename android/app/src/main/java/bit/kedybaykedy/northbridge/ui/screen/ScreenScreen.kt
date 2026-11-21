package bit.kedybaykedy.northbridge.ui.screen

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlin.math.tan

private val Black = Color(0xFF000000)
private val FieldBlack = Color(0xFF000000)
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

private fun Modifier.frameBorder(): Modifier = drawBehind {
    drawRect(color = White, style = Stroke(width = 3.dp.toPx()))
    val inset = 6.dp.toPx()
    drawRect(
        color = White,
        topLeft = Offset(inset, inset),
        size = Size(size.width - inset * 2f, size.height - inset * 2f),
        style = Stroke(width = 1.dp.toPx()),
    )
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
            text = (if (alias.isEmpty()) "MASAÜSTÜ" else alias) + " · " + stateLabel,
            style = TextStyle(
                fontSize = 11.sp,
                letterSpacing = 2.sp,
                color = Gray7,
            ),
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        )
        Text(
            text = "UZAK EKRAN",
            style = TextStyle(
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
fun ScreenScreen(viewModel: ScreenViewModel = viewModel()) {
    val uiState by viewModel.uiState.collectAsState()
    val frame by viewModel.frame.collectAsState()
    val meta by viewModel.meta.collectAsState()
    val alias by viewModel.deviceAlias.collectAsState()
    val mono by viewModel.mono.collectAsState()
    val error by viewModel.error.collectAsState()

    val stateLabel = when (uiState) {
        ScreenUiState.LIVE -> "AKIŞ"
        ScreenUiState.WAITING -> "BEKLENİYOR"
        ScreenUiState.IDLE -> "KAPALI"
    }
    val monoFilter = remember {
        ColorFilter.colorMatrix(ColorMatrix().apply { setToSaturation(0f) })
    }
    val hint = when (uiState) {
        ScreenUiState.WAITING -> "Masaüstü yanıtı bekleniyor…"
        else -> "BAŞLAT düğmesine bas, masaüstü burada görünür"
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
                .weight(1f)
                .fillMaxWidth()
                .background(FieldBlack)
                .frameBorder()
                .padding(10.dp),
        ) {
            val image = frame?.image
            if (image != null) {
                Image(
                    bitmap = image,
                    contentDescription = "Masaüstü akışı",
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Fit,
                    colorFilter = if (mono) monoFilter else null,
                )
            } else {
                Box(Modifier.fillMaxSize()) {
                    HalftoneBand(Modifier.fillMaxSize())
                    Box(
                        Modifier
                            .align(Alignment.Center)
                            .background(Panel)
                            .padding(horizontal = 20.dp, vertical = 16.dp),
                    ) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Text(
                                text = "YAYIN " + stateLabel,
                                style = TextStyle(
                                    fontSize = 11.sp,
                                    letterSpacing = 2.sp,
                                    color = Gray7,
                                ),
                            )
                            Spacer(Modifier.height(8.dp))
                            Text(
                                text = hint,
                                style = TextStyle(
                                    fontSize = 12.sp,
                                    color = Gray8,
                                ),
                            )
                        }
                    }
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        Text(
            text = if (meta.isEmpty()) "—" else meta,
            style = TextStyle(
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
                    fontSize = 12.sp,
                    color = White,
                ),
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
                enabled = uiState == ScreenUiState.IDLE,
                onClick = viewModel::start,
                modifier = Modifier.weight(1f),
            )
            SkewButton(
                label = "DURDUR",
                primary = false,
                enabled = uiState != ScreenUiState.IDLE,
                onClick = viewModel::stop,
                modifier = Modifier.weight(1f),
            )
        }
        Spacer(Modifier.height(12.dp))
        SkewButton(
            label = if (mono) "GRİ TON: AÇIK" else "GRİ TON: KAPALI",
            primary = false,
            enabled = true,
            onClick = { viewModel.setMono(!mono) },
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
