package bit.kedybaykedy.northbridge.data

import kotlinx.serialization.Serializable

@Serializable
data class TrustedDevice(
    val fingerprint: String,
    val alias: String,
    val host: String,
    val httpPort: Int = 53317,
    val pairedAt: Long = 0L,
    val incoming: List<String> = emptyList(),
    val outgoing: List<String> = emptyList(),
) {
    val packetPort: Int
        get() = httpPort + 1
}

data class DiscoveredDevice(
    val host: String,
    val port: Int,
    val alias: String,
    val fingerprint: String?,
    val version: String = "",
    val deviceType: String? = null,
    val lastSeen: Long = 0L,
) {
    val key: String
        get() = "$host:$port"
}
