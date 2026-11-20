package bit.kedybaykedy.northbridge.transport

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

@Serializable
sealed class InputMsg {

    @Serializable
    @SerialName("enter")
    data class Enter(val x: Double, val y: Double) : InputMsg()

    @Serializable
    @SerialName("leave")
    data object Leave : InputMsg()

    @Serializable
    @SerialName("move")
    data class Move(val x: Double, val y: Double) : InputMsg()

    @Serializable
    @SerialName("button")
    data class Button(val b: MouseButton, val down: Boolean) : InputMsg()

    @Serializable
    @SerialName("scroll")
    data class Scroll(val dx: Double, val dy: Double) : InputMsg()

    @Serializable
    @SerialName("key")
    data class Key(val code: Int, val down: Boolean) : InputMsg()

    @Serializable
    @SerialName("rel_move")
    data class RelMove(val dx: Double, val dy: Double) : InputMsg()

    @Serializable
    @SerialName("tap")
    data class Tap(val b: MouseButton, val count: Int) : InputMsg()

    @Serializable
    @SerialName("click_abs")
    data class ClickAbs(val x: Double, val y: Double) : InputMsg()

    @Serializable
    @SerialName("ping")
    data object Ping : InputMsg()

    @Serializable
    @SerialName("pong")
    data object Pong : InputMsg()

    @Serializable
    @SerialName("type")
    data class Type(val s: String) : InputMsg()

    @Serializable
    @SerialName("capture")
    data object Capture : InputMsg()

    @Serializable
    @SerialName("release")
    data object Release : InputMsg()
}

@Serializable
enum class MouseButton {
    @SerialName("left")
    LEFT,

    @SerialName("right")
    RIGHT,

    @SerialName("middle")
    MIDDLE,
}

@Serializable
data class NetworkPacket(
    val id: Long = 0L,
    @SerialName("type")
    val kind: String,
    val body: JsonElement = JsonObject(emptyMap()),
)

@Serializable
data class MulticastAnnouncement(
    val alias: String,
    val version: String = "",
    @SerialName("deviceModel")
    val deviceModel: String? = null,
    @SerialName("deviceType")
    val deviceType: String? = null,
    val fingerprint: String = "",
    val port: Int = 53317,
    val protocol: String = "https",
    val download: Boolean = false,
    val announce: Boolean = false,
)

@Serializable
data class InfoDto(
    val alias: String,
    val version: String = "",
    @SerialName("deviceModel")
    val deviceModel: String? = null,
    @SerialName("deviceType")
    val deviceType: String? = null,
    val fingerprint: String = "",
    val download: Boolean = false,
)

@Serializable
data class DeviceDto(
    val alias: String,
    val version: String,
    @SerialName("deviceModel")
    val deviceModel: String? = null,
    @SerialName("deviceType")
    val deviceType: String? = null,
    val fingerprint: String,
    val port: Int,
    val protocol: String,
    val download: Boolean = false,
    val pin: String? = null,
)
