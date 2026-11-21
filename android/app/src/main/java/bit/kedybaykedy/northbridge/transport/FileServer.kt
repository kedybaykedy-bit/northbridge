package bit.kedybaykedy.northbridge.transport

import android.content.ContentValues
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Log
import bit.kedybaykedy.northbridge.AppGraph
import java.io.BufferedReader
import java.io.File
import java.io.IOException
import java.io.InputStreamReader
import java.io.RandomAccessFile
import java.net.SocketException
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import javax.net.ssl.SSLServerSocket
import javax.net.ssl.SSLSocket
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

sealed class FileEvent {
    data class Received(val name: String, val size: Long, val location: String) : FileEvent()
    data class Failed(val reason: String) : FileEvent()
}

object FileServer {

    const val FILE_PORT = 53322

    const val KIND_OFFER = "nb.file.offer"
    const val KIND_CHUNK = "nb.file.chunk"
    const val KIND_DONE = "nb.file.done"

    private const val CHUNK_BYTES = 48 * 1024
    private const val MAX_FILE_BYTES = 512L * 1024L * 1024L
    private const val STALE_MS = 10L * 60L * 1000L

    private val json = Json { ignoreUnknownKeys = true }

    private val _events = MutableSharedFlow<FileEvent>(extraBufferCapacity = 32)
    val events: SharedFlow<FileEvent> = _events.asSharedFlow()

    private val running = AtomicBoolean(false)
    private var serverSocket: SSLServerSocket? = null
    private var acceptThread: Thread? = null
    private var pool: ExecutorService? = null
    private val lock = Any()
    private val transfers = HashMap<String, ActiveTransfer>()

    private data class ActiveTransfer(
        val name: String,
        val mime: String,
        val size: Long,
        val count: Long,
        val part: File,
        val out: RandomAccessFile,
        var received: Long = 0L,
        var at: Long = System.currentTimeMillis(),
    )

    @Synchronized
    fun start() {
        if (running.get()) return
        val trust = Tls.trustingFingerprints {
            AppGraph.deviceStore.devices.value.map { it.fingerprint }.toSet()
        }
        val factory = Tls.serverSocketFactory(AppGraph.identity, trust)
        val socket = factory.createServerSocket(FILE_PORT) as SSLServerSocket
        socket.needClientAuth = true
        serverSocket = socket
        val executor = Executors.newCachedThreadPool()
        pool = executor
        running.set(true)
        val thread = Thread({
            Log.d("northbridge", "file server listening on $FILE_PORT")
            while (running.get()) {
                try {
                    val connection = socket.accept() as SSLSocket
                    executor.execute { serve(connection) }
                } catch (e: SocketException) {
                    if (running.get()) Log.w("northbridge", "file server accept failed: $e")
                    break
                } catch (e: IOException) {
                    if (!running.get() || socket.isClosed) break
                    Log.w("northbridge", "file server accept failed: $e")
                } catch (e: Exception) {
                    if (!running.get() || socket.isClosed) break
                    Log.w("northbridge", "file server accept failed: $e")
                }
            }
            Log.d("northbridge", "file server stopped")
        }, "nb-file-server")
        thread.isDaemon = true
        acceptThread = thread
        thread.start()
    }

    fun ensureRunning() {
        if (!running.get()) {
            try {
                start()
            } catch (e: Exception) {
                Log.w("northbridge", "file server start failed: $e")
            }
        }
    }

    @Synchronized
    fun stop() {
        if (!running.getAndSet(false)) return
        try {
            serverSocket?.close()
        } catch (_: Exception) {
        }
        serverSocket = null
        acceptThread = null
        try {
            pool?.shutdownNow()
        } catch (_: Exception) {
        }
        pool = null
    }

    fun isRunning(): Boolean = running.get()

    private fun serve(connection: SSLSocket) {
        val remote = try {
            connection.remoteSocketAddress?.toString() ?: "unknown"
        } catch (_: Exception) {
            "unknown"
        }
        try {
            connection.startHandshake()
            val fingerprint = connection.session.peerCertificates
                ?.firstOrNull()
                ?.let { sha256Upper(it.encoded) }
                ?: ""
            Log.d("northbridge", "file server accepted $remote (${fingerprint.take(12)})")
            val reader = BufferedReader(InputStreamReader(connection.inputStream, Charsets.UTF_8))
            val output = connection.outputStream
            while (true) {
                val line = try {
                    reader.readLine()
                } catch (_: IOException) {
                    break
                } ?: break
                val packet = try {
                    json.decodeFromString<NetworkPacket>(line)
                } catch (_: Exception) {
                    null
                } ?: continue
                val response = handle(packet, fingerprint) ?: continue
                val payload = json.encodeToString(NetworkPacket.serializer(), response) + "\n"
                val bytes = payload.toByteArray(Charsets.UTF_8)
                synchronized(output) {
                    output.write(bytes)
                    output.flush()
                }
            }
        } catch (e: Exception) {
            Log.w("northbridge", "file server session $remote ended: $e")
        } finally {
            try {
                connection.close()
            } catch (_: Exception) {
            }
        }
    }

    private fun paired(fingerprint: String): Boolean = try {
        AppGraph.deviceStore.devices.value.any { it.fingerprint.equals(fingerprint, ignoreCase = true) }
    } catch (_: Exception) {
        false
    }

    private fun handle(packet: NetworkPacket, peerFingerprint: String): NetworkPacket {
        val now = System.currentTimeMillis()
        if (!paired(peerFingerprint)) {
            return when (packet.kind) {
                KIND_OFFER -> NetworkPacket(now, KIND_OFFER, buildJsonObject {
                    put("accept", false)
                    put("reason", "not paired")
                })
                else -> NetworkPacket(now, packet.kind, buildJsonObject {
                    put("ok", false)
                    put("error", "not paired")
                })
            }
        }
        return when (packet.kind) {
            KIND_OFFER -> onOffer(packet.body, now)
            KIND_CHUNK -> onChunk(packet.body, now)
            KIND_DONE -> onDone(packet.body, now)
            else -> NetworkPacket(now, packet.kind, buildJsonObject {
                put("ok", false)
                put("error", "unknown packet type: ${packet.kind}")
            })
        }
    }

    private fun onOffer(body: JsonElement, now: Long): NetworkPacket {
        val obj = body as? JsonObject ?: return decline(now, "bozuk teklif")
        val rawName = (obj["name"] as? JsonPrimitive)?.content ?: return decline(now, "isim yok")
        val size = (obj["size"] as? JsonPrimitive)?.content?.toLongOrNull()
            ?: return decline(now, "boyut yok")
        val mime = (obj["mime"] as? JsonPrimitive)?.content?.takeIf { it.isNotBlank() }
            ?: "application/octet-stream"
        if (size < 0 || size > MAX_FILE_BYTES) return decline(now, "boyut kabul edilmiyor")
        val name = sanitizeFileName(rawName)
        val count = if (size == 0L) 0L else (size + CHUNK_BYTES - 1) / CHUNK_BYTES
        return try {
            sweepStale()
            val id = java.util.UUID.randomUUID().toString()
            val part = File(AppGraph.appContext.cacheDir, "nb-file-$id.part")
            if (part.exists()) part.delete()
            val out = RandomAccessFile(part, "rw")
            if (size > 0) {
                try {
                    out.setLength(size)
                } catch (_: Exception) {
                }
            }
            synchronized(lock) {
                transfers[id] = ActiveTransfer(name, mime, size, count, part, out)
            }
            Log.d("northbridge", "file offer accepted $name ($size bytes)")
            NetworkPacket(now, KIND_OFFER, buildJsonObject {
                put("accept", true)
                put("transfer", id)
            })
        } catch (e: Exception) {
            Log.w("northbridge", "file offer failed: $e")
            decline(now, "kaydedilemiyor")
        }
    }

    private fun decline(now: Long, reason: String): NetworkPacket =
        NetworkPacket(now, KIND_OFFER, buildJsonObject {
            put("accept", false)
            put("reason", reason)
        })

    private fun onChunk(body: JsonElement, now: Long): NetworkPacket {
        val obj = body as? JsonObject ?: return chunkFail(now, "bozuk parça")
        val id = (obj["transfer"] as? JsonPrimitive)?.content ?: return chunkFail(now, "oturum yok")
        val index = (obj["index"] as? JsonPrimitive)?.content?.toLongOrNull()
            ?: return chunkFail(now, "sıra yok")
        val dataB64 = (obj["data"] as? JsonPrimitive)?.content ?: return chunkFail(now, "veri yok")
        if (index < 0) return chunkFail(now, "sıra geçersiz")
        val data = try {
            java.util.Base64.getDecoder().decode(dataB64)
        } catch (_: Exception) {
            return chunkFail(now, "veri çözülemedi")
        }
        if (data.size > CHUNK_BYTES + 16) return chunkFail(now, "parça çok büyük")
        val t = synchronized(lock) { transfers[id] } ?: return chunkFail(now, "oturum yok")
        val offset = index * CHUNK_BYTES
        if (offset + data.size > t.size) return chunkFail(now, "dosya sonu aşıldı")
        return try {
            synchronized(t) {
                t.out.seek(offset)
                t.out.write(data)
                t.received += data.size
                t.at = System.currentTimeMillis()
            }
            NetworkPacket(now, KIND_CHUNK, buildJsonObject {
                put("ok", true)
                put("index", index)
            })
        } catch (e: Exception) {
            chunkFail(now, "yazılamadı")
        }
    }

    private fun chunkFail(now: Long, reason: String): NetworkPacket =
        NetworkPacket(now, KIND_CHUNK, buildJsonObject {
            put("ok", false)
            put("error", reason)
        })

    private fun onDone(body: JsonElement, now: Long): NetworkPacket {
        val obj = body as? JsonObject ?: return doneFail(now, "bozuk kapanış")
        val id = (obj["transfer"] as? JsonPrimitive)?.content ?: return doneFail(now, "oturum yok")
        val count = (obj["count"] as? JsonPrimitive)?.content?.toLongOrNull()
            ?: return doneFail(now, "sayı yok")
        val t = synchronized(lock) { transfers.remove(id) } ?: return doneFail(now, "oturum yok")
        try {
            synchronized(t) {
                t.out.close()
            }
            val actual = t.part.length()
            if (t.size == 0L) {
                if (actual != 0L || count != 0L) {
                    t.part.delete()
                    return doneFail(now, "eksik dosya")
                }
            } else {
                if (actual < t.size || count != t.count) {
                    t.part.delete()
                    return doneFail(now, "eksik dosya")
                }
            }
            val location = saveToDownloads(t.name, t.mime, t.part)
            t.part.delete()
            _events.tryEmit(FileEvent.Received(t.name, t.size, location))
            Log.d("northbridge", "file received ${t.name} -> $location")
            return NetworkPacket(now, KIND_DONE, buildJsonObject {
                put("ok", true)
                put("name", t.name)
            })
        } catch (e: Exception) {
            try {
                t.part.delete()
            } catch (_: Exception) {
            }
            Log.w("northbridge", "file receive failed: $e")
            _events.tryEmit(FileEvent.Failed("alınamadı: ${t.name}"))
            return doneFail(now, "kaydedilemedi")
        }
    }

    private fun doneFail(now: Long, reason: String): NetworkPacket =
        NetworkPacket(now, KIND_DONE, buildJsonObject {
            put("ok", false)
            put("error", reason)
        })

    private fun saveToDownloads(name: String, mime: String, part: File): String {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            mediaStoreSave(name, mime, part)
        } else {
            legacySave(name, part)
        }
    }

    private fun mediaStoreSave(name: String, mime: String, part: File): String {
        val resolver = AppGraph.appContext.contentResolver
        val unique = uniqueDownloadName(name)
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, unique)
            put(MediaStore.Downloads.MIME_TYPE, mime)
            put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
        }
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: throw IOException("indirilenler açılamadı")
        resolver.openOutputStream(uri)?.use { out ->
            part.inputStream().use { it.copyTo(out) }
        } ?: throw IOException("yazılamadı")
        return "İndirilenler/$unique"
    }

    private fun legacySave(name: String, part: File): String {
        val dir = AppGraph.appContext.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
            ?: throw IOException("klasör açılamadı")
        if (!dir.exists() && !dir.mkdirs()) throw IOException("klasör açılamadı")
        val unique = uniqueFileName(dir, sanitizeFileName(name))
        part.inputStream().use { input ->
            File(dir, unique).outputStream().use { input.copyTo(it) }
        }
        return "Uygulama/Download/$unique"
    }

    private fun uniqueDownloadName(name: String): String {
        val resolver = AppGraph.appContext.contentResolver
        val existing = HashSet<String>()
        try {
            resolver.query(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                arrayOf(MediaStore.Downloads.DISPLAY_NAME),
                null, null, null,
            )?.use { cursor ->
                val column = cursor.getColumnIndex(MediaStore.Downloads.DISPLAY_NAME)
                if (column >= 0) {
                    while (cursor.moveToNext()) {
                        cursor.getString(column)?.let { existing.add(it) }
                    }
                }
            }
        } catch (_: Exception) {
        }
        return dedup(name, existing::contains)
    }

    private fun uniqueFileName(dir: File, name: String): String {
        return dedup(name) { File(dir, it).exists() }
    }

    private fun dedup(name: String, exists: (String) -> Boolean): String {
        if (!exists(name)) return name
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val ext = if (dot > 0) name.substring(dot) else ""
        for (i in 1..999) {
            val candidate = "$stem ($i)$ext"
            if (!exists(candidate)) return candidate
        }
        return "$stem-${System.currentTimeMillis()}$ext"
    }

    private fun sweepStale() {
        val cutoff = System.currentTimeMillis() - STALE_MS
        synchronized(lock) {
            val dead = transfers.filterValues { it.at < cutoff }.keys.toList()
            for (id in dead) {
                val t = transfers.remove(id) ?: continue
                try {
                    t.out.close()
                } catch (_: Exception) {
                }
                try {
                    t.part.delete()
                } catch (_: Exception) {
                }
            }
        }
    }

    fun sanitizeFileName(raw: String): String {
        val base = raw.substringAfterLast('/').substringAfterLast('\\').trim()
        val cleaned = base.filter { !it.isISOControl() }.take(255)
        return cleaned.ifEmpty { "dosya" }
    }
}
