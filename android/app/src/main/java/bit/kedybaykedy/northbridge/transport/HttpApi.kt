package bit.kedybaykedy.northbridge.transport

import android.util.Log
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

data class ProbeResult(
    val alias: String,
    val version: String,
    val deviceType: String?,
    val fingerprint: String,
)

object HttpApi {

    private val json = Json { ignoreUnknownKeys = true }

    private fun client(identity: PhoneIdentity, trust: javax.net.ssl.X509TrustManager): OkHttpClient =
        OkHttpClient.Builder()
            .sslSocketFactory(Tls.socketFactory(identity, trust), trust)
            .hostnameVerifier { _, _ -> true }
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(5, TimeUnit.SECONDS)
            .retryOnConnectionFailure(true)
            .build()

    private fun url(host: String, port: Int, path: String): String {
        val bracketed = if (host.contains(":") && !host.startsWith("[")) "[$host]" else host
        return "https://$bracketed:$port$path"
    }

    suspend fun probe(host: String, port: Int, identity: PhoneIdentity): ProbeResult? =
        withContext(Dispatchers.IO) {
            val client = client(identity, Tls.trustAll())
            val request = Request.Builder()
                .url(url(host, port, "/api/localsend/v2/info"))
                .get()
                .build()
            try {
                Log.d("northbridge", "probe GET $host:$port")
                client.newCall(request).execute().use { response ->
                    if (!response.isSuccessful) return@withContext null
                    val body = response.body?.string() ?: return@withContext null
                    val info = json.decodeFromString<InfoDto>(body)
                    val handshakeFingerprint = response.handshake
                        ?.peerCertificates
                        ?.firstOrNull()
                        ?.let { sha256Upper(it.encoded) }
                    ProbeResult(
                        alias = info.alias,
                        version = info.version,
                        deviceType = info.deviceType,
                        fingerprint = handshakeFingerprint ?: info.fingerprint,
                    )
                }
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (e: IOException) {
                Log.e("northbridge", "probe failed", e)
                null
            } catch (e: Exception) {
                Log.e("northbridge", "probe error", e)
                null
            }
        }

    suspend fun pairHttp(
        host: String,
        port: Int,
        identity: PhoneIdentity,
        dto: DeviceDto,
    ): Boolean = withContext(Dispatchers.IO) {
        val client = client(identity, Tls.trustAll())
        val body = json.encodeToString(dto).toRequestBody("application/json".toMediaType())
        val request = Request.Builder()
            .url(url(host, port, "/api/requests/pair"))
            .post(body)
            .build()
        try {
            client.newCall(request).execute().use { it.isSuccessful }
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (_: Exception) {
            false
        }
    }
}
