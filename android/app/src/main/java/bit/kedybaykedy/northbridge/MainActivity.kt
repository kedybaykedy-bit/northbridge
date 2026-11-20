package bit.kedybaykedy.northbridge

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import bit.kedybaykedy.northbridge.ui.NorthbridgeRoot
import bit.kedybaykedy.northbridge.ui.theme.NorthbridgeTheme

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            NorthbridgeTheme {
                NorthbridgeRoot()
            }
        }
    }
}
