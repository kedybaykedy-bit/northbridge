package bit.kedybaykedy.northbridge.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import bit.kedybaykedy.northbridge.R

val AntonFamily = FontFamily(Font(R.font.anton_regular, FontWeight.Normal))

val ArchivoBlackFamily = FontFamily(Font(R.font.archivo_black_regular, FontWeight.Normal))

val CourierPrimeFamily = FontFamily(
    Font(R.font.courier_prime_regular, FontWeight.Normal),
    Font(R.font.courier_prime_bold, FontWeight.Bold),
)

val MmBlack = Color(0xFF000000)
val MmPanel = Color(0xFF0A0A0A)
val MmRaised = Color(0xFF1A1A1A)
val MmGhost = Color(0xFF262626)
val MmShadow = Color(0xFF4A4A4A)
val MmRing = Color(0xFF6A6A6A)
val MmMeta = Color(0xFF8A8A8A)
val MmKicker = Color(0xFF9A9A9A)
val MmSecondary = Color(0xFFC0C0C0)
val MmBody = Color(0xFFCFCFCF)
val MmInk = Color(0xFFF2F2F2)

private val NorthbridgeColorScheme = darkColorScheme(
    primary = MmInk,
    onPrimary = MmPanel,
    primaryContainer = MmInk,
    onPrimaryContainer = MmBlack,
    secondary = MmSecondary,
    onSecondary = MmBlack,
    secondaryContainer = MmGhost,
    onSecondaryContainer = MmInk,
    tertiary = MmKicker,
    onTertiary = MmBlack,
    background = MmBlack,
    onBackground = MmInk,
    surface = MmPanel,
    onSurface = MmInk,
    surfaceVariant = MmRaised,
    onSurfaceVariant = MmSecondary,
    outline = MmShadow,
    outlineVariant = MmRing,
    error = MmMeta,
    onError = MmBlack,
)

private val NorthbridgeTypography = Typography(
    displayLarge = TextStyle(
        fontFamily = AntonFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 44.sp,
        lineHeight = 44.sp,
    ),
    displayMedium = TextStyle(
        fontFamily = AntonFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 32.sp,
        lineHeight = 36.sp,
    ),
    titleLarge = TextStyle(
        fontFamily = AntonFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 24.sp,
        lineHeight = 29.sp,
    ),
    titleMedium = TextStyle(
        fontFamily = AntonFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 16.sp,
        lineHeight = 22.sp,
    ),
    labelLarge = TextStyle(
        fontFamily = ArchivoBlackFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 11.sp,
        lineHeight = 16.sp,
    ),
    labelMedium = TextStyle(
        fontFamily = ArchivoBlackFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 10.sp,
        lineHeight = 14.sp,
    ),
    bodyLarge = TextStyle(
        fontFamily = CourierPrimeFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 15.sp,
        lineHeight = 26.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = CourierPrimeFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 14.sp,
        lineHeight = 24.sp,
    ),
    bodySmall = TextStyle(
        fontFamily = CourierPrimeFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 12.sp,
        lineHeight = 18.sp,
    ),
)

@Composable
fun NorthbridgeTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = NorthbridgeColorScheme,
        typography = NorthbridgeTypography,
        content = content,
    )
}
