package bit.kedybaykedy.northbridge.transport

import android.os.Build
import bit.kedybaykedy.northbridge.data.TrustedDevice
import java.io.IOException
import java.io.InputStream
import java.net.URLEncoder
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okio.BufferedSink

object FileClient {

    const val MAX_FILE_BYTES: Long = 512L * 1024L * 1024L

    private val json = Json { ignoreUnknownKeys = true }

    private fun client(identity: PhoneIdentity, trust: javax.net.ssl.X509TrustManager): OkHttpClient =
        OkHttpClient.Builder()
            .sslSocketFactory(Tls.socketFactory(identity, trust), trust)
            .hostnameVerifier { _, _ -> true }
            .connectTimeout(5, TimeUnit.SECONDS)
            .retryOnConnectionFailure(false)
            .build()

    private fun baseUrl(device: TrustedDevice, path: String): String {
        val host = device.host
        val bracketed = if (host.contains(":") && !host.startsWith("[")) "[$host]" else host
        return "https://$bracketed:${device.httpPort}/api/localsend/v2$path"
    }

    private fun enc(value: String): String = URLEncoder.encode(value, "UTF-8")

    suspend fun upload(
        device: TrustedDevice,
        identity: PhoneIdentity,
        fileName: String,
        mime: String,
        size: Long,
        open: () -> InputStream,
        onProgress: (sent: Long, total: Long) -> Unit = { _, _ -> },
    ): String = withContext(Dispatchers.IO) {
        if (size < 0) throw IOException("dosya boyutu okunamadı")
        if (size > MAX_FILE_BYTES) {
            throw IOException("dosya çok büyük (${size / 1048576} MB, sınır 512 MB)")
        }
        val trust = Tls.pinned(device.fingerprint)
        val http = client(identity, trust)
        val fileId = UUID.randomUUID().toString()
        val safeName = sanitizeFileName(fileName)
        val prepareBody = buildJsonObject {
            put("info", buildJsonObject {
                put("alias", JsonPrimitive(identity.alias))
                put("version", JsonPrimitive("2.2"))
                put("deviceModel", JsonPrimitive(Build.MODEL ?: "Android"))
                put("deviceType", JsonPrimitive("mobile"))
                put("fingerprint", JsonPrimitive(identity.fingerprint))
                put("port", JsonPrimitive(53317))
                put("protocol", JsonPrimitive("https"))
                put("download", JsonPrimitive(false))
            })
            put("files", buildJsonObject {
                put(fileId, buildJsonObject {
                    put("id", JsonPrimitive(fileId))
                    put("fileName", JsonPrimitive(safeName))
                    put("size", JsonPrimitive(size))
                    put("fileType", JsonPrimitive(mime.ifEmpty { "application/octet-stream" }))
                })
            })
        }.toString()
        val prepareRequest = Request.Builder()
            .url(baseUrl(device, "/prepare-upload"))
            .post(prepareBody.toRequestBody("application/json".toMediaType()))
            .build()
        val prepared = try {
            http.newCall(prepareRequest).execute().use { response ->
                when (response.code) {
                    200 -> {
                        val text = response.body?.string() ?: throw IOException("boş prepare yanıtı")
                        json.parseToJsonElement(text) as? JsonObject
                            ?: throw IOException("prepare yanıtı çözülemedi")
                    }
                    204 -> throw IOException("bilgisayar aktarımı kabul etmedi")
                    401 -> throw IOException("bilgisayar PIN istiyor")
                    403 -> throw IOException("bilgisayar reddetti (eşleşme yok?)")
                    409 -> throw IOException("bilgisayar meşgul, sonra tekrar dene")
                    429 -> throw IOException("çok fazla istek, sonra tekrar dene")
                    else -> throw IOException("prepare-upload HTTP ${response.code}")
                }
            }
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (e: IOException) {
            throw e
        } catch (e: Exception) {
            throw IOException("ulaşılamıyor: ${e.message}")
        }
        val sessionId = (prepared["sessionId"] as? JsonPrimitive)?.content
            ?: throw IOException("oturum açılamadı")
        val files = prepared["files"] as? JsonObject
            ?: throw IOException("oturum açılamadı")
        val token = (files[fileId] as? JsonPrimitive)?.content
            ?: throw IOException("bilgisayar dosyayı kabul etmedi")
        val url = baseUrl(device, "/upload") +
            "?sessionId=${enc(sessionId)}&fileId=${enc(fileId)}&token=${enc(token)}"
        val body = streamBody(mime.ifEmpty { "application/octet-stream" }, size, open, onProgress)
        val uploadRequest = Request.Builder().url(url).post(body).build()
        try {
            http.newCall(uploadRequest).execute().use { response ->
                if (!response.isSuccessful) throw IOException("yükleme HTTP ${response.code}")
            }
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (e: IOException) {
            throw e
        } catch (e: Exception) {
            throw IOException("yükleme koptu: ${e.message}")
        }
        safeName
    }

    private fun streamBody(
        mime: String,
        size: Long,
        open: () -> InputStream,
        onProgress: (Long, Long) -> Unit,
    ): RequestBody = object : RequestBody() {
        override fun contentType() = mime.toMediaType()
        override fun contentLength(): Long = size
        override fun writeTo(sink: BufferedSink) {
            open().use { input ->
                val buffer = ByteArray(64 * 1024)
                var sent = 0L
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    sink.write(buffer, 0, read)
                    sent += read
                    onProgress(sent, size)
                }
                sink.flush()
            }
        }
    }

    fun sanitizeFileName(raw: String): String {
        val base = raw.substringAfterLast('/').substringAfterLast('\\').trim()
        val cleaned = base.filter { !it.isISOControl() }.take(255)
        return cleaned.ifEmpty { "dosya" }
    }
}
