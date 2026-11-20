package bit.kedybaykedy.northbridge.pairing

import bit.kedybaykedy.northbridge.transport.DeviceDto
import bit.kedybaykedy.northbridge.transport.HttpApi
import bit.kedybaykedy.northbridge.transport.NetworkPacket
import bit.kedybaykedy.northbridge.transport.PacketChannel
import bit.kedybaykedy.northbridge.transport.PhoneIdentity
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

data class PairOutcome(
    val accepted: Boolean,
    val alias: String,
    val incoming: List<String>,
    val outgoing: List<String>,
    val via: String,
)

object PairingClient {

    private const val PROTOCOL_VERSION = "2.2"
    private const val CAPABILITY_IN = "nb.kvm"
    private const val CAPABILITY_OUT = "nb.touchpad"

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    suspend fun pair(
        host: String,
        httpPort: Int,
        fingerprint: String,
        identity: PhoneIdentity,
        pin: String,
    ): PairOutcome {
        val viaHttp = tryPairHttp(host, httpPort, identity, pin)
        if (viaHttp != null) return viaHttp
        return pairOverPacket(host, httpPort + 1, fingerprint, identity, pin)
    }

    private suspend fun tryPairHttp(
        host: String,
        httpPort: Int,
        identity: PhoneIdentity,
        pin: String,
    ): PairOutcome? {
        val dto = DeviceDto(
            alias = identity.alias,
            version = PROTOCOL_VERSION,
            deviceModel = "northbridge",
            deviceType = "mobile",
            fingerprint = identity.fingerprint,
            port = 53317,
            protocol = "https",
            download = false,
            pin = pin,
        )
        val accepted = HttpApi.pairHttp(host, httpPort, identity, dto)
        if (!accepted) return null
        return PairOutcome(
            accepted = true,
            alias = host,
            incoming = emptyList(),
            outgoing = emptyList(),
            via = "http",
        )
    }

    private suspend fun pairOverPacket(
        host: String,
        packetPort: Int,
        fingerprint: String,
        identity: PhoneIdentity,
        pin: String,
    ): PairOutcome {
        val now = System.currentTimeMillis()
        val body = buildJsonObject {
            put("protocol", 1)
            put("alias", identity.alias)
            put("timestamp", now)
            put(
                "incoming",
                JsonArray(listOf(JsonPrimitive(CAPABILITY_IN))),
            )
            put(
                "outgoing",
                JsonArray(listOf(JsonPrimitive(CAPABILITY_OUT))),
            )
            put("pin", pin)
        }
        val request = NetworkPacket(id = now, kind = "nb.pair", body = body)
        val response = try {
            PacketChannel.request(host, packetPort, identity, fingerprint, request)
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (e: IOException) {
            throw IOException("pairing over packet channel failed: ${e.message}", e)
        }
        val responseBody = response.body as? JsonObject ?: JsonObject(emptyMap())
        return PairOutcome(
            accepted = readBool(responseBody, "accepted") ?: false,
            alias = readString(responseBody, "alias") ?: host,
            incoming = readStringList(responseBody, "incoming"),
            outgoing = readStringList(responseBody, "outgoing"),
            via = "packet",
        )
    }

    private fun readBool(obj: JsonObject, key: String): Boolean? {
        val primitive = obj[key] as? JsonPrimitive ?: return null
        if (primitive === JsonNull) return null
        return when (primitive.content) {
            "true" -> true
            "false" -> false
            else -> null
        }
    }

    private fun readString(obj: JsonObject, key: String): String? {
        val primitive = obj[key] as? JsonPrimitive ?: return null
        if (primitive === JsonNull) return null
        return primitive.content
    }

    private fun readStringList(obj: JsonObject, key: String): List<String> {
        val array = obj[key] as? JsonArray ?: return emptyList()
        return array.mapNotNull { element: JsonElement ->
            val primitive = element as? JsonPrimitive ?: return@mapNotNull null
            if (primitive === JsonNull) null else primitive.content
        }
    }
}
