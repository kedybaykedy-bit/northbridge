package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.ui.theme.MmPanel

@Composable
fun TouchpadScreen(viewModel: TouchpadViewModel = viewModel()) {
    val connection by AppGraph.inputChannel.state.collectAsState()

    Column(
        Modifier
            .fillMaxSize()
            .background(MmPanel),
    ) {
        ConnectionBanner(connection)
        KeyboardPanel(
            viewModel = viewModel,
            modifier = Modifier
                .weight(1f)
                .padding(horizontal = 12.dp),
        )
        Spacer(Modifier.height(12.dp))
    }
}
