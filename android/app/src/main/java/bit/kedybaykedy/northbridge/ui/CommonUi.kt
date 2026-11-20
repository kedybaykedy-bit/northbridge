package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import bit.kedybaykedy.northbridge.transport.ConnectionState

@Composable
fun ConnectionBanner(connection: ConnectionState, modifier: Modifier = Modifier) {
    val (label, color) = when (connection) {
        ConnectionState.CONNECTED -> "Connected" to MaterialTheme.colorScheme.secondary
        ConnectionState.CONNECTING -> "Connecting…" to MaterialTheme.colorScheme.tertiary
        ConnectionState.RECONNECTING -> "Reconnecting…" to MaterialTheme.colorScheme.tertiary
        ConnectionState.DISCONNECTED -> "Disconnected" to MaterialTheme.colorScheme.error
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(10.dp)
                .clip(CircleShape)
                .background(color),
        )
        Spacer(Modifier.width(8.dp))
        Text(label, style = MaterialTheme.typography.bodyMedium)
    }
}
