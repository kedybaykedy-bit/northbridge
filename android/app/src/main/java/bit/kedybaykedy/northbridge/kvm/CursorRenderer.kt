package bit.kedybaykedy.northbridge.kvm

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke

data class CursorPos(
    val x: Float,
    val y: Float,
)

fun DrawScope.drawDesktopCursor(at: Offset) {
    val path = Path().apply {
        moveTo(at.x, at.y)
        lineTo(at.x, at.y + 30f)
        lineTo(at.x + 7.5f, at.y + 22.5f)
        lineTo(at.x + 12.5f, at.y + 32f)
        lineTo(at.x + 16.5f, at.y + 30f)
        lineTo(at.x + 11.5f, at.y + 20.5f)
        lineTo(at.x + 20f, at.y + 19.5f)
        close()
    }
    drawPath(path, color = Color(0xFFF2F2F2))
    drawPath(path, color = Color(0xFF0A0A0A), style = Stroke(width = 1.5f))
}
