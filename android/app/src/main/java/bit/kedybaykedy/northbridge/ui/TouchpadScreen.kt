package bit.kedybaykedy.northbridge.ui

import android.content.res.Configuration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.input.touchpadGestures
import bit.kedybaykedy.northbridge.transport.ConnectionState
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import bit.kedybaykedy.northbridge.ui.theme.MmInk
import bit.kedybaykedy.northbridge.ui.theme.MmKicker
import bit.kedybaykedy.northbridge.ui.theme.MmMeta
import bit.kedybaykedy.northbridge.ui.theme.MmPanel

@Composable
fun TouchpadScreen(viewModel: TouchpadViewModel = viewModel()) {
    val connection by AppGraph.inputChannel.state.collectAsState()
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    var landscapeKeyboardVisible by rememberSaveable { mutableStateOf(true) }

    Column(
        Modifier
            .fillMaxSize()
            .background(MmPanel),
    ) {
        ConnectionBanner(connection)
        if (isLandscape) {
            if (landscapeKeyboardVisible) {
                LandscapeKeyboardPanel(
                    viewModel = viewModel,
                    onToggle = { landscapeKeyboardVisible = !landscapeKeyboardVisible },
                    modifier = Modifier.weight(1f),
                )
            } else {
                LandscapeHiddenPanel(
                    viewModel = viewModel,
                    onToggle = { landscapeKeyboardVisible = !landscapeKeyboardVisible },
                    modifier = Modifier.weight(1f),
                )
            }
        } else {
            PureTouchpadArea(
                viewModel = viewModel,
                connection = connection,
                modifier = Modifier
                    .weight(1f)
                    .padding(horizontal = 12.dp),
            )
            Spacer(Modifier.height(8.dp))
            KeyboardPanel(
                viewModel = viewModel,
                modifier = Modifier.padding(horizontal = 12.dp),
            )
            Spacer(Modifier.height(12.dp))
        }
    }
}

@Composable
private fun PureTouchpadArea(
    viewModel: TouchpadViewModel,
    connection: ConnectionState,
    modifier: Modifier = Modifier,
) {
    val ctrlLatched by viewModel.ctrlLatched.collectAsState()
    val altLatched by viewModel.altLatched.collectAsState()
    val shiftLatched by viewModel.shiftLatched.collectAsState()
    val symLatched by viewModel.symLatched.collectAsState()

    Box(
        modifier = modifier
            .fillMaxWidth()
            .background(MmPanel)
            .border(width = 2.dp, color = MmInk)
            .touchpadGestures(viewModel::send),
    ) {
        HalftoneDots(Modifier.matchParentSize())
        Column(
            Modifier
                .fillMaxSize()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = "TOUCHPAD · BAS = TIK · SURUKLE = FARE",
                style = TextStyle(
                    fontFamily = ArchivoBlackFamily,
                    fontSize = 11.sp,
                    letterSpacing = 3.sp,
                    color = MmKicker,
                ),
            )
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f, fill = false)
                    .heightIn(min = 48.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = padHint(connection, ctrlLatched, altLatched, shiftLatched, symLatched),
                    style = TextStyle(
                        fontFamily = CourierPrimeFamily,
                        fontSize = 11.sp,
                        color = MmMeta,
                    ),
                )
            }
            MmDoubleRule()
            Text(
                text = panelStatus(connection, ctrlLatched, altLatched, shiftLatched, symLatched),
                style = TextStyle(
                    fontFamily = CourierPrimeFamily,
                    fontSize = 11.sp,
                    color = MmMeta,
                ),
            )
        }
    }
}
