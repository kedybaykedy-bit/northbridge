package bit.kedybaykedy.northbridge.transport

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import bit.kedybaykedy.northbridge.AppGraph
import java.io.BufferedReader
import java.io.IOException
import java.io.InputStreamReader
import java.net.SocketException
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import javax.net.ssl.SSLServerSocket
import javax.net.ssl.SSLSocket
import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.addJsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

object PacketServer {

    const val PACKET_PORT = 53318

    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    private val running = AtomicBoolean(false)
    private var serverSocket: SSLServerSocket? = null
    private var acceptThread: Thread? = null
    private var pool: ExecutorService? = null
    private val mainHandler = Handler(Looper.getMainLooper())

    @Synchronized
    fun start() {
        if (running.get()) return
        val trust = Tls.trustingFingerprints {
            AppGraph.deviceStore.devices.value.map { it.fingerprint }.toSet()
        }
        val factory = Tls.serverSocketFactory(AppGraph.identity, trust)
        val socket = factory.createServerSocket(PACKET_PORT) as SSLServerSocket
        socket.needClientAuth = true
        serverSocket = socket
        val executor = Executors.newCachedThreadPool()
        pool = executor
        running.set(true)
        restoreKeepAwake()
        val thread = Thread({
            Log.d("northbridge", "packet server listening on $PACKET_PORT")
            while (running.get()) {
                try {
                    val connection = socket.accept() as SSLSocket
                    executor.execute { serve(connection) }
                } catch (e: SocketException) {
                    if (running.get()) {
                        Log.w("northbridge", "packet server accept failed: $e")
                    }
                    break
                } catch (e: IOException) {
                    if (!running.get() || socket.isClosed) break
                    Log.w("northbridge", "packet server accept failed: $e")
                } catch (e: Exception) {
                    if (!running.get() || socket.isClosed) break
                    Log.w("northbridge", "packet server accept failed: $e")
                }
            }
            Log.d("northbridge", "packet server stopped")
        }, "nb-packet-server")
        thread.isDaemon = true
        acceptThread = thread
        thread.start()
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
                ?: "none"
            Log.d("northbridge", "packet server accepted $remote (${fingerprint.take(12)})")
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
                } catch (_: SerializationException) {
                    null
                } catch (_: IllegalArgumentException) {
                    null
                } ?: continue
                val response = handle(packet, fingerprint) ?: continue
                val payload = json.encodeToString(response) + "\n"
                val bytes = payload.toByteArray(Charsets.UTF_8)
                synchronized(output) {
                    output.write(bytes)
                    output.flush()
                }
            }
            Log.d("northbridge", "packet server closed $remote")
        } catch (e: Exception) {
            Log.w("northbridge", "packet server session $remote ended: $e")
        } finally {
            try {
                connection.close()
            } catch (_: Exception) {
            }
        }
    }

    private fun handle(packet: NetworkPacket, peerFingerprint: String): NetworkPacket? {
        val now = System.currentTimeMillis()
        return when (packet.kind) {
            KIND_PING -> {
                val message = ((packet.body as? JsonObject)?.get("message") as? JsonPrimitive)?.content
                    ?: "ping"
                NetworkPacket(
                    id = now,
                    kind = KIND_PONG,
                    body = buildJsonObject {
                        put("message", message)
                        put("reply", AppGraph.identity.alias)
                        put("channel", "lan")
                        put("at", now)
                    },
                )
            }
            KIND_CLIPBOARD -> {
                val content = ((packet.body as? JsonObject)?.get("content") as? JsonPrimitive)?.content
                    ?: return NetworkPacket(
                        id = now,
                        kind = KIND_ERROR,
                        body = buildJsonObject {
                            put("reason", "missing content")
                        },
                    )
                copyToClipboard(content)
                NetworkPacket(
                    id = now,
                    kind = KIND_CLIPBOARD,
                    body = buildJsonObject {
                        put("ack", true)
                    },
                )
            }
            KIND_CONTROL -> handleControl(packet.body, peerFingerprint, now)
            else -> NetworkPacket(
                id = now,
                kind = KIND_ERROR,
                body = buildJsonObject {
                    put("reason", "unknown packet type: ${packet.kind}")
                },
            )
        }
    }

    private fun copyToClipboard(text: String) {
        try {
            val clipboard = AppGraph.appContext.getSystemService(ClipboardManager::class.java)
                ?: return
            val clip = ClipData.newPlainText("northbridge", text)
            if (Looper.myLooper() == Looper.getMainLooper()) {
                clipboard.setPrimaryClip(clip)
            } else {
                mainHandler.post { clipboard.setPrimaryClip(clip) }
            }
            Log.d("northbridge", "packet server clipboard applied (${text.length} chars)")
        } catch (e: Exception) {
            Log.w("northbridge", "packet server clipboard failed: $e")
        }
    }

    private data class SuResult(val code: Int, val out: String)

    private fun runSu(script: String, timeoutMs: Long = 20000L): SuResult {
        return try {
            val proc = Runtime.getRuntime().exec(arrayOf("su", "-c", script + " 2>&1; echo NB_RC=$?"))
            val text = proc.inputStream.bufferedReader(Charsets.UTF_8).readText()
            val done = proc.waitFor(timeoutMs, TimeUnit.MILLISECONDS)
            if (!done) {
                try {
                    proc.destroyForcibly()
                } catch (_: Exception) {
                }
                SuResult(124, text.trim())
            } else {
                val lines = text.trim().lines()
                val last = lines.lastOrNull().orEmpty()
                if (last.startsWith("NB_RC=")) {
                    val code = last.removePrefix("NB_RC=").toIntOrNull() ?: proc.exitValue()
                    SuResult(code, lines.dropLast(1).joinToString("\n").trim())
                } else {
                    SuResult(proc.exitValue(), text.trim())
                }
            }
        } catch (e: Exception) {
            SuResult(127, "su failed: ${e.message}")
        }
    }

    private fun shQuote(s: String): String = "'" + s.replace("'", "'\\''") + "'"

    private data class PowerState(val awake: Boolean, val keyguard: Boolean, val keyguardExists: Boolean)

    private fun readPowerState(): PowerState {
        val r = runSu("dumpsys power | grep mWakefulness=; dumpsys window | grep -E 'isKeyguardShowing=|deviceHasKeyguard='")
        var awake = false
        var keyguard = false
        var exists = false
        for (line in r.out.lines()) {
            if (line.contains("mWakefulness=Awake")) awake = true
            if (line.contains("isKeyguardShowing=true")) keyguard = true
            if (line.contains("deviceHasKeyguard=true")) exists = true
        }
        return PowerState(awake, keyguard, exists)
    }

    private fun controlError(now: Long, action: String, reason: String): NetworkPacket {
        return NetworkPacket(
            id = now,
            kind = KIND_CONTROL,
            body = buildJsonObject {
                put("ok", false)
                put("action", action)
                put("error", reason)
            },
        )
    }

    private fun handleControl(body: JsonElement, peerFingerprint: String, now: Long): NetworkPacket {
        val obj = body as? JsonObject ?: return controlError(now, "?", "malformed body")
        val t = (obj["t"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            ?: return controlError(now, "?", "missing t")
        val paired = try {
            AppGraph.deviceStore.devices.value.any { it.fingerprint.equals(peerFingerprint, ignoreCase = true) }
        } catch (_: Exception) {
            false
        }
        if (!paired) return controlError(now, t, "not paired")
        return when (t) {
            "wake" -> doWake(now)
            "sleep" -> doSleep(now)
            "open" -> {
                val pkg = (obj["pkg"] as? JsonPrimitive)?.takeIf { it.isString }?.content
                    ?: return controlError(now, "open", "missing pkg")
                if (!PKG_RE.matches(pkg)) return controlError(now, "open", "bad package name")
                doOpen(now, pkg)
            }
            "keepawake" -> {
                val onEl = obj["on"] as? JsonPrimitive
                    ?: return controlError(now, "keepawake", "missing on")
                doKeepAwake(now, onEl.content == "true")
            }
            "apps" -> doApps(now)
            else -> controlError(now, t, "unknown control")
        }
    }

    private fun wakeAndDismiss() {
        runSu("input keyevent KEYCODE_WAKEUP")
        try {
            Thread.sleep(400)
        } catch (_: InterruptedException) {
        }
        runSu("wm dismiss-keyguard")
        try {
            Thread.sleep(400)
        } catch (_: InterruptedException) {
        }
    }

    private fun doWake(now: Long): NetworkPacket {
        wakeAndDismiss()
        val st = readPowerState()
        Log.d("northbridge", "control wake awake=${st.awake} keyguard=${st.keyguard}")
        return NetworkPacket(
            id = now,
            kind = KIND_CONTROL,
            body = buildJsonObject {
                put("ok", st.awake)
                put("action", "wake")
                put("awake", st.awake)
                put("keyguard", st.keyguard)
                if (!st.awake) {
                    put("error", "display did not wake")
                } else if (st.keyguard) {
                    put("note", "keyguard still showing; unlock on the phone to proceed")
                }
            },
        )
    }

    private fun doSleep(now: Long): NetworkPacket {
        runSu("input keyevent KEYCODE_SLEEP")
        try {
            Thread.sleep(500)
        } catch (_: InterruptedException) {
        }
        val st = readPowerState()
        Log.d("northbridge", "control sleep awake=${st.awake}")
        return NetworkPacket(
            id = now,
            kind = KIND_CONTROL,
            body = buildJsonObject {
                put("ok", !st.awake)
                put("action", "sleep")
                put("awake", st.awake)
                if (st.awake) put("error", "display still awake")
            },
        )
    }

    private fun doOpen(now: Long, pkg: String): NetworkPacket {
        wakeAndDismiss()
        var component: String? = null
        val resolved = runSu("cmd package resolve-activity --brief " + shQuote(pkg))
        for (line in resolved.out.lines().map { it.trim() }.filter { it.isNotEmpty() }.asReversed()) {
            if (line.contains("/") && !line.contains(" ") && CMP_RE.matches(line)) {
                component = line
                break
            }
        }
        var method: String? = null
        var error: String? = null
        if (component != null) {
            val s = runSu("am start -n " + shQuote(component))
            val started = (s.out.contains("Status: ok") || s.out.contains("Starting:")) && !s.out.contains("Error")
            if (started) {
                method = "am"
            } else {
                error = s.out.lines().firstOrNull { it.contains("Error") }?.trim()
                    ?: "am start failed (rc=${s.code})"
                component = null
            }
        } else {
            error = "no launcher activity for $pkg"
        }
        if (method == null) {
            val m = runSu("monkey -p " + shQuote(pkg) + " -c android.intent.category.LAUNCHER 1 1>/dev/null")
            if (m.code == 0 && !m.out.contains("No activities found")) {
                method = "monkey"
                component = pkg
                error = null
            } else if (error == null) {
                error = "no launcher activity for $pkg"
            }
        }
        val st = readPowerState()
        Log.d("northbridge", "control open pkg=$pkg method=$method error=$error")
        return NetworkPacket(
            id = now,
            kind = KIND_CONTROL,
            body = buildJsonObject {
                put("ok", method != null)
                put("action", "open")
                put("pkg", pkg)
                if (method != null) {
                    put("component", component ?: pkg)
                    put("method", method)
                } else {
                    put("error", error ?: "start failed")
                }
                put("awake", st.awake)
                put("keyguard", st.keyguard)
            },
        )
    }

    private fun doKeepAwake(now: Long, on: Boolean): NetworkPacket {
        val prefs = AppGraph.prefs
        if (on) {
            if (prefs.getBoolean(PREF_KEEP, false)) {
                return NetworkPacket(
                    id = now,
                    kind = KIND_CONTROL,
                    body = buildJsonObject {
                        put("ok", true)
                        put("action", "keepawake")
                        put("on", true)
                        put("note", "already on")
                    },
                )
            }
            val prevStayon = runSu("settings get global stay_on_while_plugged_in").out.lines().firstOrNull().orEmpty().trim()
            val prevTimeout = runSu("settings get system screen_off_timeout").out.lines().firstOrNull().orEmpty().trim()
            prefs.edit()
                .putBoolean(PREF_KEEP, true)
                .putString(PREF_STAYON, prevStayon)
                .putString(PREF_TIMEOUT, prevTimeout)
                .apply()
            runSu("svc power stayon true")
            runSu("settings put system screen_off_timeout 2147483647")
            wakeAndDismiss()
            val st = readPowerState()
            Log.d("northbridge", "control keepawake on prev_stayon=$prevStayon prev_timeout=$prevTimeout")
            return NetworkPacket(
                id = now,
                kind = KIND_CONTROL,
                body = buildJsonObject {
                    put("ok", true)
                    put("action", "keepawake")
                    put("on", true)
                    put("prev_stayon", prevStayon)
                    put("prev_timeout", prevTimeout)
                    put("awake", st.awake)
                },
            )
        }
        val prevStayon = prefs.getString(PREF_STAYON, null)
        val prevTimeout = prefs.getString(PREF_TIMEOUT, null)
        if (prevStayon != null && NUM_RE.matches(prevStayon)) {
            runSu("settings put global stay_on_while_plugged_in $prevStayon")
        }
        if (prevTimeout != null && NUM_RE.matches(prevTimeout)) {
            runSu("settings put system screen_off_timeout $prevTimeout")
        }
        prefs.edit().putBoolean(PREF_KEEP, false).remove(PREF_STAYON).remove(PREF_TIMEOUT).apply()
        Log.d("northbridge", "control keepawake off restored stayon=$prevStayon timeout=$prevTimeout")
        return NetworkPacket(
            id = now,
            kind = KIND_CONTROL,
            body = buildJsonObject {
                put("ok", true)
                put("action", "keepawake")
                put("on", false)
            },
        )
    }

    private fun restoreKeepAwake() {
        try {
            val prefs = AppGraph.prefs
            if (!prefs.getBoolean(PREF_KEEP, false)) return
            val stayon = prefs.getString(PREF_STAYON, null)
            val timeout = prefs.getString(PREF_TIMEOUT, null)
            if (stayon != null && NUM_RE.matches(stayon)) {
                runSu("settings put global stay_on_while_plugged_in $stayon")
            }
            if (timeout != null && NUM_RE.matches(timeout)) {
                runSu("settings put system screen_off_timeout $timeout")
            }
            prefs.edit().putBoolean(PREF_KEEP, false).remove(PREF_STAYON).remove(PREF_TIMEOUT).apply()
            Log.d("northbridge", "control keepawake restored after restart")
        } catch (e: Exception) {
            Log.w("northbridge", "control keepawake restore failed: $e")
        }
    }

    private data class LaunchApp(val pkg: String, val label: String)

    private fun doApps(now: Long): NetworkPacket {
        val found = try {
            val pm = AppGraph.appContext.packageManager
            val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
            val infos = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                pm.queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(0))
            } else {
                @Suppress("DEPRECATION")
                pm.queryIntentActivities(intent, 0)
            }
            infos.mapNotNull { info ->
                val pkg = info.activityInfo?.packageName ?: return@mapNotNull null
                if (!PKG_RE.matches(pkg)) return@mapNotNull null
                val label = try {
                    info.loadLabel(pm)?.toString()?.trim().orEmpty().ifEmpty { pkg }
                } catch (_: Exception) {
                    pkg
                }
                LaunchApp(pkg, label)
            }
                .distinctBy { it.pkg }
                .sortedWith(compareBy({ it.label.lowercase() }, { it.pkg }))
        } catch (e: Exception) {
            return controlError(now, "apps", "query failed: ${e.message}")
        }
        val truncated = found.size > MAX_APPS
        val page = if (truncated) found.take(MAX_APPS) else found
        Log.d("northbridge", "control apps total=${found.size} returned=${page.size}")
        return NetworkPacket(
            id = now,
            kind = KIND_CONTROL,
            body = buildJsonObject {
                put("ok", true)
                put("action", "apps")
                put("total", found.size)
                put("truncated", truncated)
                put("apps", buildJsonArray {
                    for (app in page) {
                        addJsonObject {
                            put("pkg", app.pkg)
                            put("label", app.label)
                        }
                    }
                })
            },
        )
    }

    private const val KIND_PING = "nb.ping"
    private const val KIND_PONG = "nb.pong"
    private const val KIND_CLIPBOARD = "nb.clipboard"
    private const val KIND_CONTROL = "nb.control"
    private const val KIND_ERROR = "nb.error"

    private const val PREF_KEEP = "nb_keepawake_on"
    private const val PREF_STAYON = "nb_keepawake_stayon"
    private const val PREF_TIMEOUT = "nb_keepawake_timeout"
    private const val MAX_APPS = 400

    private val PKG_RE = Regex("^[A-Za-z][A-Za-z0-9_.]*$")
    private val CMP_RE = Regex("^[A-Za-z][A-Za-z0-9_.]*/[A-Za-z0-9_.\$]+$")
    private val NUM_RE = Regex("^[0-9]+$")
}
