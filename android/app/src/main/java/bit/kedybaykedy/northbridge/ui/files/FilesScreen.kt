package bit.kedybaykedy.northbridge.ui.files

import android.content.Intent
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
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
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.ui.theme.AntonFamily
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
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
            .height(56.dp)
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
private fun Masthead(alias: String) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            text = (if (alias.isEmpty()) "MASAÜSTÜ SEÇİLMEDİ" else alias) + " · DOSYA",
            style = TextStyle(
                fontFamily = ArchivoBlackFamily,
                fontSize = 11.sp,
                letterSpacing = 2.sp,
                color = Gray7,
            ),
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        )
        Text(
            text = "DOSYA KÖPRÜSÜ",
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

private fun formatSize(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    if (bytes < 1048576) return "%d KB".format(bytes / 1024)
    return "%.1f MB".format(bytes / 1048576.0)
}

private fun formatTime(at: Long): String {
    return try {
        SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date(at))
    } catch (_: Exception) {
        ""
    }
}

@Composable
fun FilesScreen(viewModel: FilesViewModel = viewModel()) {
    val alias by viewModel.deviceAlias.collectAsState()
    val sending by viewModel.sending.collectAsState()
    val status by viewModel.status.collectAsState()
    val error by viewModel.error.collectAsState()
    val received by viewModel.received.collectAsState()

    val context = LocalContext.current
    val picker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri != null) {
            try {
                context.contentResolver.takePersistableUriPermission(
                    uri,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION,
                )
            } catch (_: Exception) {
            }
            viewModel.send(uri)
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(Black)
            .padding(16.dp)
            .verticalScroll(rememberScrollState()),
    ) {
        Masthead(alias = alias)
        Spacer(Modifier.height(12.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .miniBox()
                .padding(horizontal = 16.dp, vertical = 14.dp),
        ) {
            Column {
                Text(
                    text = "TELEFON → BİLGİSAYAR",
                    style = TextStyle(
                        fontFamily = ArchivoBlackFamily,
                        fontSize = 10.sp,
                        letterSpacing = 2.sp,
                        color = Gray7,
                    ),
                )
                Spacer(Modifier.height(10.dp))
                SkewButton(
                    label = if (sending) "GÖNDERİLİYOR…" else "DOSYA GÖNDER",
                    primary = true,
                    enabled = !sending,
                    onClick = { picker.launch(arrayOf("*/*")) },
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    text = status,
                    style = TextStyle(
                        fontFamily = CourierPrimeFamily,
                        fontSize = 12.sp,
                        color = Gray8,
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
                } else if (alias.isEmpty()) {
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = "ÖNCE CİHAZLAR SEKMESİNDE BİR MASAÜSTÜ SEÇ",
                        style = TextStyle(
                            fontFamily = CourierPrimeFamily,
                            fontSize = 12.sp,
                            color = Gray6,
                        ),
                    )
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        HalftoneBand(Modifier.fillMaxWidth().height(24.dp))
        Spacer(Modifier.height(10.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .miniBox()
                .padding(horizontal = 16.dp, vertical = 14.dp),
        ) {
            Column {
                Text(
                    text = "BİLGİSAYAR → TELEFON · İNDİRİLENLER",
                    style = TextStyle(
                        fontFamily = ArchivoBlackFamily,
                        fontSize = 10.sp,
                        letterSpacing = 2.sp,
                        color = Gray7,
                    ),
                )
                Spacer(Modifier.height(10.dp))
                if (received.isEmpty()) {
                    Text(
                        text = "HENÜZ DOSYA ALINMADI",
                        style = TextStyle(
                            fontFamily = CourierPrimeFamily,
                            fontSize = 12.sp,
                            color = Gray6,
                        ),
                    )
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = "BİLGİSAYARDAN: northbridge send <cihaz> <dosya>",
                        style = TextStyle(
                            fontFamily = CourierPrimeFamily,
                            fontSize = 11.sp,
                            color = Gray6,
                        ),
                    )
                } else {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        received.forEach { file ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                            ) {
                                Text(
                                    text = file.name,
                                    style = TextStyle(
                                        fontFamily = CourierPrimeFamily,
                                        fontSize = 13.sp,
                                        color = White,
                                    ),
                                    modifier = Modifier.weight(1f),
                                )
                                Text(
                                    text = formatSize(file.size) + " · " + formatTime(file.at),
                                    style = TextStyle(
                                        fontFamily = CourierPrimeFamily,
                                        fontSize = 11.sp,
                                        color = Gray7,
                                    ),
                                )
                            }
                            Text(
                                text = file.location,
                                style = TextStyle(
                                    fontFamily = CourierPrimeFamily,
                                    fontSize = 11.sp,
                                    color = Gray6,
                                ),
                            )
                        }
                    }
                }
            }
        }
    }
}
