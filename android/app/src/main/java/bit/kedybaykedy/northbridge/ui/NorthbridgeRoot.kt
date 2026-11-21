package bit.kedybaykedy.northbridge.ui

import android.content.res.Configuration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cast
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Monitor
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material.icons.filled.TouchApp
import androidx.compose.material.icons.filled.VolumeUp
import bit.kedybaykedy.northbridge.ui.audio.AudioScreen
import bit.kedybaykedy.northbridge.ui.files.FilesScreen
import bit.kedybaykedy.northbridge.ui.mirror.MirrorScreen
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation.NavController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import bit.kedybaykedy.northbridge.ui.screen.ScreenScreen
import bit.kedybaykedy.northbridge.ui.theme.AntonFamily
import bit.kedybaykedy.northbridge.ui.theme.ArchivoBlackFamily
import bit.kedybaykedy.northbridge.ui.theme.CourierPrimeFamily
import bit.kedybaykedy.northbridge.ui.theme.MmBlack
import bit.kedybaykedy.northbridge.ui.theme.MmInk
import bit.kedybaykedy.northbridge.ui.theme.MmKicker
import bit.kedybaykedy.northbridge.ui.theme.MmMeta
import bit.kedybaykedy.northbridge.ui.theme.MmPanel
import bit.kedybaykedy.northbridge.ui.theme.MmSecondary
import bit.kedybaykedy.northbridge.ui.theme.MmShadow
import java.util.Locale

object Routes {
    const val DISCOVERY = "discovery"
    const val TOUCHPAD = "touchpad"
    const val KVM = "kvm"
    const val SCREEN = "screen"
    const val MIRROR = "mirror"
    const val AUDIO = "audio"
    const val FILES = "files"
    const val DEVICES = "devices"
}

private data class TopLevelDestination(
    val route: String,
    val label: String,
    val icon: ImageVector,
)

private val topLevelDestinations = listOf(
    TopLevelDestination(Routes.DISCOVERY, "Discover", Icons.Filled.Search),
    TopLevelDestination(Routes.TOUCHPAD, "Touchpad", Icons.Filled.TouchApp),
    TopLevelDestination(Routes.KVM, "KVM", Icons.Filled.Computer),
    TopLevelDestination(Routes.SCREEN, "Screen", Icons.Filled.Monitor),
    TopLevelDestination(Routes.MIRROR, "Mirror", Icons.Filled.Cast),
    TopLevelDestination(Routes.AUDIO, "Audio", Icons.Filled.VolumeUp),
    TopLevelDestination(Routes.FILES, "Files", Icons.Filled.Folder),
    TopLevelDestination(Routes.DEVICES, "Devices", Icons.Filled.Smartphone),
)

private fun navigateTopLevel(navController: NavController, route: String) {
    navController.navigate(route) {
        popUpTo(navController.graph.startDestinationId) {
            saveState = true
        }
        launchSingleTop = true
        restoreState = true
    }
}

@Composable
fun NorthbridgeRoot() {
    val navController = rememberNavController()
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = backStackEntry?.destination?.route
    val configuration = LocalConfiguration.current
    val isLandscape =
        configuration.orientation == Configuration.ORIENTATION_LANDSCAPE ||
            configuration.screenWidthDp > configuration.screenHeightDp

    Scaffold(
        bottomBar = {
            if (!isLandscape) {
                Column(modifier = Modifier.fillMaxWidth()) {
                    HorizontalDoubleRule()
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(MmPanel)
                            .horizontalScroll(rememberScrollState())
                            .padding(vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        topLevelDestinations.forEach { destination ->
                            RailItem(
                                selected = currentRoute == destination.route,
                                onClick = { navigateTopLevel(navController, destination.route) },
                                icon = destination.icon,
                                label = destination.label.uppercase(Locale.ROOT),
                                modifier = Modifier,
                                iconSize = 26.dp,
                            )
                        }
                    }
                }
            }
        },
    ) { padding ->
        Row(
            modifier = Modifier.fillMaxSize().padding(padding),
        ) {
            if (isLandscape) {
                MastheadRail(
                    currentRoute = currentRoute,
                    onDestinationClick = { route -> navigateTopLevel(navController, route) },
                    modifier = Modifier.fillMaxHeight(),
                )
            }
            Box(
                modifier = Modifier.weight(1f).fillMaxHeight(),
            ) {
                NavHost(
                    navController = navController,
                    startDestination = Routes.DISCOVERY,
                    modifier = Modifier.fillMaxSize(),
                ) {
                    composable(Routes.DISCOVERY) { DiscoveryScreen() }
                    composable(Routes.TOUCHPAD) { TouchpadScreen() }
                    composable(Routes.KVM) { KvmScreen() }
                    composable(Routes.SCREEN) { ScreenScreen() }
                    composable(Routes.MIRROR) { MirrorScreen() }
                    composable(Routes.AUDIO) { AudioScreen() }
                    composable(Routes.FILES) { FilesScreen() }
                    composable(Routes.DEVICES) { DevicesScreen() }
                }
            }
        }
    }
}

@Composable
private fun MastheadRail(
    currentRoute: String?,
    onDestinationClick: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val scroll = rememberScrollState()
    Row(
        modifier = modifier.background(MmPanel),
    ) {
        Column(
            modifier = Modifier
                .width(96.dp)
                .fillMaxHeight()
                .verticalScroll(scroll)
                .selectableGroup()
                .padding(vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = "NB",
                color = MmInk,
                style = MaterialTheme.typography.displayMedium.copy(
                    fontFamily = AntonFamily,
                    fontStyle = FontStyle.Italic,
                    shadow = Shadow(offset = Offset(2f, 2f), color = MmShadow),
                ),
                modifier = Modifier.skewX(-6f),
            )
            Text(
                text = "NAV",
                color = MmKicker,
                fontFamily = ArchivoBlackFamily,
                fontSize = 10.sp,
                letterSpacing = 2.8.sp,
                modifier = Modifier.padding(top = 2.dp),
            )
            HorizontalDoubleRule(modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp))
            topLevelDestinations.forEach { destination ->
                RailItem(
                    selected = currentRoute == destination.route,
                    onClick = { onDestinationClick(destination.route) },
                    icon = destination.icon,
                    label = destination.label.uppercase(Locale.ROOT),
                )
            }
            Text(
                text = "S.01-" + topLevelDestinations.size.toString().padStart(2, '0'),
                color = MmMeta,
                fontFamily = CourierPrimeFamily,
                fontSize = 11.sp,
                modifier = Modifier.padding(top = 12.dp, bottom = 4.dp),
            )
        }
        VerticalDoubleRule()
    }
}

@Composable
private fun RailItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: ImageVector,
    label: String,
    modifier: Modifier = Modifier.fillMaxWidth(),
    labelSize: TextUnit = 10.sp,
    labelTracking: TextUnit = 0.8.sp,
    iconSize: Dp = 22.dp,
) {
    Column(
        modifier = modifier
            .selectable(selected = selected, role = Role.Tab, onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            contentAlignment = Alignment.Center,
        ) {
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .offset(3.dp, 3.dp)
                    .skewX(-10f)
                    .background(if (selected) MmShadow else Color.Transparent),
            )
            Box(
                modifier = Modifier
                    .skewX(-10f)
                    .background(if (selected) MmInk else Color.Transparent)
                    .border(1.5.dp, if (selected) MmInk else MmShadow)
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = label,
                    tint = if (selected) MmBlack else MmSecondary,
                    modifier = Modifier.size(iconSize).skewX(10f),
                )
            }
        }
        Spacer(modifier = Modifier.height(6.dp))
        Text(
            text = label,
            color = if (selected) MmInk else MmSecondary,
            fontFamily = ArchivoBlackFamily,
            fontSize = labelSize,
            letterSpacing = labelTracking,
            textAlign = TextAlign.Center,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
        )
        if (selected) {
            Box(
                modifier = Modifier.padding(top = 4.dp).width(24.dp).height(2.dp).background(MmInk),
            )
        }
    }
}

@Composable
private fun HorizontalDoubleRule(modifier: Modifier = Modifier) {
    Column(modifier = modifier.fillMaxWidth()) {
        Box(modifier = Modifier.fillMaxWidth().height(3.dp).background(MmInk))
        Spacer(modifier = Modifier.height(2.dp))
        Box(modifier = Modifier.fillMaxWidth().height(1.dp).background(MmInk))
    }
}

@Composable
private fun VerticalDoubleRule(modifier: Modifier = Modifier) {
    Row(modifier = modifier.fillMaxHeight()) {
        Box(modifier = Modifier.width(3.dp).fillMaxHeight().background(MmInk))
        Spacer(modifier = Modifier.width(2.dp))
        Box(modifier = Modifier.width(1.dp).fillMaxHeight().background(MmInk))
    }
}
