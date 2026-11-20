package bit.kedybaykedy.northbridge.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material.icons.filled.TouchApp
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController

object Routes {
    const val DISCOVERY = "discovery"
    const val TOUCHPAD = "touchpad"
    const val KVM = "kvm"
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
    TopLevelDestination(Routes.DEVICES, "Devices", Icons.Filled.Smartphone),
)

@Composable
fun NorthbridgeRoot() {
    val navController = rememberNavController()
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = backStackEntry?.destination?.route

    Scaffold(
        bottomBar = {
            NavigationBar {
                topLevelDestinations.forEach { destination ->
                    NavigationBarItem(
                        selected = currentRoute == destination.route,
                        onClick = {
                            navController.navigate(destination.route) {
                                popUpTo(navController.graph.startDestinationId) {
                                    saveState = true
                                }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = { Icon(destination.icon, contentDescription = destination.label) },
                        label = { Text(destination.label) },
                    )
                }
            }
        },
    ) { padding ->
        NavHost(
            navController = navController,
            startDestination = Routes.DISCOVERY,
            modifier = Modifier.padding(padding),
        ) {
            composable(Routes.DISCOVERY) { DiscoveryScreen() }
            composable(Routes.TOUCHPAD) { TouchpadScreen() }
            composable(Routes.KVM) { KvmScreen() }
            composable(Routes.DEVICES) { DevicesScreen() }
        }
    }
}
