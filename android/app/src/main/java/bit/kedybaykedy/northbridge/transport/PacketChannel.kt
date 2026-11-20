package bit.kedybaykedy.northbridge.transport

import java.io.IOException
import java.net.InetSocketAddress
import javax.net.ssl.SSLSocket
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

object PacketChannel {

    private const val KIND_ERROR = "nb.error"

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    suspend fun request(
        host: String,
        packetPort: Int,
        identity: PhoneIdentity,
        expectedFingerprint: String?,
        packet: NetworkPacket,
        timeoutMs: Int = 5000,
    ): NetworkPacket = withContext(Dispatchers.IO) {
        val trust = expectedFingerprint?.let { Tls.pinned(it) } ?: Tls.trustAll()
        val factory = Tls.socketFactory(identity, trust)
        val socket = factory.createSocket() as SSLSocket
        try {
            socket.connect(InetSocketAddress(host, packetPort), timeoutMs)
            socket.soTimeout = timeoutMs
            socket.startHandshake()
            val line = json.encodeToString(packet) + "\n"
            socket.outputStream.write(line.toByteArray(Charsets.UTF_8))
            socket.outputStream.flush()
            val responseLine = socket.inputStream.bufferedReader(Charsets.UTF_8).readLine()
                ?: throw IOException("peer closed without a response")
            val response = try {
                json.decodeFromString<NetworkPacket>(responseLine)
            } catch (_: SerializationException) {
                throw IOException("malformed response line")
            } catch (_: IllegalArgumentException) {
                throw IOException("malformed response line")
            }
            if (response.kind == KIND_ERROR) {
                val reason = (response.body as? JsonObject)?.get("reason") as? JsonPrimitive
                throw IOException(reason?.content ?: KIND_ERROR)
            }
            response
        } catch (cancellation: CancellationException) {
            throw cancellation
        } finally {
            try {
                socket.close()
            } catch (_: Exception) {
            }
        }
    }
}
