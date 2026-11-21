package bit.kedybaykedy.northbridge.discovery

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log
import bit.kedybaykedy.northbridge.transport.MulticastAnnouncement
import java.net.DatagramPacket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.MulticastSocket
import java.net.NetworkInterface
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

data class HeardAnnouncement(
    val host: String,
    val message: MulticastAnnouncement,
)

class MulticastDiscovery(private val context: Context) {

    private val json = Json { ignoreUnknownKeys = true }
    private var lock: WifiManager.MulticastLock? = null
    private var socket: MulticastSocket? = null

    fun start(scope: CoroutineScope, onAnnouncement: (HeardAnnouncement) -> Unit) {
        if (socket != null) return
        scope.launch(Dispatchers.IO) {
            if (socket != null) return@launch
            Log.d("northbridge", "discovery start")
            val wifi = context.applicationContext
                .getSystemService(Context.WIFI_SERVICE) as WifiManager
            val multicastLock = wifi.createMulticastLock("northbridge-discovery")
            multicastLock.setReferenceCounted(false)
            multicastLock.acquire()
            lock = multicastLock

            val multicastSocket = try {
                MulticastSocket(DISCOVERY_PORT).apply {
                    soTimeout = RECEIVE_TIMEOUT_MS
                }
            } catch (e: Exception) {
                Log.e("northbridge", "discovery socket failed", e)
                multicastLock.release()
                return@launch
            }
            socket = multicastSocket
            Log.d("northbridge", "discovery socket ready")

            val group = InetAddress.getByName(GROUP_ADDRESS)
            joinInterfaces(multicastSocket, group)

            val buffer = ByteArray(BUFFER_SIZE)
            while (isActive) {
                val packet = DatagramPacket(buffer, buffer.size)
                try {
                    multicastSocket.receive(packet)
                } catch (_: java.net.SocketTimeoutException) {
                    continue
                } catch (_: Exception) {
                    if (!isActive) break
                    delay(200)
                    continue
                }
                val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                val message = try {
                    json.decodeFromString<MulticastAnnouncement>(text)
                } catch (_: Exception) {
                    continue
                }
                val host = packet.address?.hostAddress ?: continue
                Log.d("northbridge", "discovery heard ${message.alias} from $host")
                onAnnouncement(HeardAnnouncement(host, message))
            }
        }
    }

    fun stop() {
        val multicastSocket = socket
        socket = null
        if (multicastSocket != null) {
            try {
                multicastSocket.leaveGroup(InetAddress.getByName(GROUP_ADDRESS))
            } catch (_: Exception) {
            }
            try {
                multicastSocket.close()
            } catch (_: Exception) {
            }
        }
        lock?.release()
        lock = null
    }

    private fun joinInterfaces(multicastSocket: MulticastSocket, group: InetAddress) {
        val target = InetSocketAddress(group, DISCOVERY_PORT)
        var joined = false
        val interfaces = try {
            NetworkInterface.getNetworkInterfaces()?.toList() ?: emptyList()
        } catch (_: Exception) {
            emptyList()
        }
        for (networkInterface in interfaces) {
            try {
                if (!networkInterface.isUp || !networkInterface.supportsMulticast()) continue
                multicastSocket.joinGroup(target, networkInterface)
                joined = true
            } catch (_: Exception) {
            }
        }
        if (!joined) {
            try {
                multicastSocket.joinGroup(group)
                joined = true
            } catch (e: Exception) {
                Log.e("northbridge", "discovery join failed: $e")
            }
        }
        Log.d("northbridge", "discovery joined=$joined")
    }

    companion object {
        const val GROUP_ADDRESS = "224.0.0.167"
        const val DISCOVERY_PORT = 53317
        private const val RECEIVE_TIMEOUT_MS = 1000
        private const val BUFFER_SIZE = 65536
    }
}
