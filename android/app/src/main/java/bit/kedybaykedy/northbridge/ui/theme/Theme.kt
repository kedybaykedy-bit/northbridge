package bit.kedybaykedy.northbridge.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val NorthbridgeColorScheme = darkColorScheme(
    primary = Color(0xFF8AB4F8),
    onPrimary = Color(0xFF002E61),
    secondary = Color(0xFF81C995),
    onSecondary = Color(0xFF00391D),
    tertiary = Color(0xFF7AABFF),
    background = Color(0xFF101418),
    onBackground = Color(0xFFE1E2E6),
    surface = Color(0xFF101418),
    onSurface = Color(0xFFE1E2E6),
    surfaceVariant = Color(0xFF44474E),
    onSurfaceVariant = Color(0xFFC4C6D0),
    outline = Color(0xFF8E9099),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
)

@Composable
fun NorthbridgeTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = NorthbridgeColorScheme,
        content = content,
    )
}
