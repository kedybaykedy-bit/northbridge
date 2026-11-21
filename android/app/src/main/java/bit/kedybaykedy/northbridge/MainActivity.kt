package bit.kedybaykedy.northbridge

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import android.content.Intent
import bit.kedybaykedy.northbridge.keepalive.PacketKeepAliveService
import bit.kedybaykedy.northbridge.ui.NorthbridgeRoot
import bit.kedybaykedy.northbridge.ui.theme.NorthbridgeTheme

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContent {
            NorthbridgeTheme {
                NorthbridgeRoot()
            }
        }
    }

    override fun onStart() {
        super.onStart()
        startForegroundService(Intent(this, PacketKeepAliveService::class.java))
    }
}
