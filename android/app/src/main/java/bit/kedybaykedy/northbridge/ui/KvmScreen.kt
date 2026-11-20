package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.kvm.drawDesktopCursor

@Composable
fun KvmScreen(viewModel: KvmViewModel = viewModel()) {
    val cursor by viewModel.cursor.collectAsState()
    val connection by AppGraph.inputChannel.state.collectAsState()

    Column(Modifier.fillMaxSize()) {
        ConnectionBanner(connection)
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .background(Color(0xFF0B0D10)),
        ) {
            Canvas(
                Modifier
                    .fillMaxSize()
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
                val position = cursor
                if (position != null) {
                    drawDesktopCursor(
                        Offset(position.x * size.width, position.y * size.height),
                    )
                }
            }
            if (cursor == null) {
                Text(
                    text = "Waiting for desktop cursor…",
                    modifier = Modifier.align(Alignment.Center),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
