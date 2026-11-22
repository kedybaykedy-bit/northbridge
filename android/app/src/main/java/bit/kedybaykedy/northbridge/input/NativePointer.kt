package bit.kedybaykedy.northbridge.input

import android.util.Log

import android.content.ComponentName
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.net.LocalSocket
import android.net.LocalSocketAddress
import android.os.IBinder
import android.os.SystemClock
import androidx.core.content.pm.PackageInfoCompat
import bit.kedybaykedy.northbridge.AppGraph
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import rikka.shizuku.Shizuku

enum class PointerMode {
    AUTO,
    REAL,
    VIRTUAL,
}

object PointerJni {
    init {
        try {
            System.loadLibrary("nbpointer")
        } catch (_: Throwable) {
        }
    }

    external fun nbCreate(): Int
    external fun nbMoveRel(dx: Int, dy: Int): Int
    external fun nbButton(code: Int, down: Boolean): Int
    external fun nbWheel(delta: Int): Int
    external fun nbDestroy()

    fun loadExplicit(path: String): Boolean {
        return try {
            System.load(path)
            true
        } catch (_: Throwable) {
            false
        }
    }

    fun extractLib(apkPath: String, outDir: File, name: String): String? {
        return try {
            if (!outDir.exists()) {
                outDir.mkdirs()
            }
            val target = File(outDir, name)
            var picked: String? = null
            try {
                val zip = java.util.zip.ZipFile(apkPath)
                try {
                    for (abi in android.os.Build.SUPPORTED_ABIS) {
                        val entry = zip.getEntry("lib/" + abi + "/libnbpointer.so")
                        if (entry != null) {
                            zip.getInputStream(entry).use { input ->
                                target.outputStream().use { output ->
                                    input.copyTo(output)
                                }
                            }
                            picked = target.absolutePath
                            break
                        }
                    }
                } finally {
                    try {
                        zip.close()
                    } catch (_: Exception) {
                    }
                }
            } catch (_: Exception) {
            }
            picked
        } catch (_: Exception) {
            null
        }
    }
}

object NativePointer {
    const val BTN_LEFT = 272
    const val BTN_RIGHT = 273
    const val BTN_MIDDLE = 274
    const val TMP_LIB = "/data/local/tmp/nbpointer_app.so"
    const val PRIV_LIB = "nbpointer.so"
    private const val POINTER_SENSITIVITY = 0.65
    private const val MIRROR_FREE_GAIN = 1.0
    private const val FLICK_LINEAR_PX = 64
    private const val FLICK_EXCESS_GAIN = 0.3
    private const val PC_W = 1920.0
    private const val PC_H = 1080.0
    private const val SHIZUKU_REQ = 4411
    private const val DAEMON_WAIT_MS = 8000L

    val mode = MutableStateFlow(PointerMode.AUTO)

    private val _status = MutableStateFlow("IMLEC: SANAL")
    val status: StateFlow<String> = _status.asStateFlow()

    private val lock = Any()
    private var backend: DaemonLink? = null
    private var shizukuArgs: Shizuku.UserServiceArgs? = null
    private var shizukuConn: ServiceConnection? = null
    private var sessionOn = false
    private var shizukuAsked = false
    private val held = mutableSetOf<Int>()
    private var posInit = false
    private var posX = 0.0
    private var posY = 0.0
    private var accX = 0.0
    private var accY = 0.0
    private var fx = 0.0
    private var fy = 0.0
    private var ex = 0
    private var ey = 0
    private var geomW = 0
    private var geomH = 0
    private var freeInit = false

    init {
        try {
            Shizuku.addRequestPermissionResultListener { requestCode, grantResult ->
                if (requestCode == SHIZUKU_REQ) {
                    if (grantResult == PackageManager.PERMISSION_GRANTED) {
                        setStatus("IMLEC: SANAL (SHIZUKU ONAYLANDI)")
                        val retry = synchronized(lock) {
                            sessionOn && mode.value != PointerMode.VIRTUAL && backend == null
                        }
                        if (retry) {
                            try {
                                Thread { sessionStart() }.start()
                            } catch (_: Exception) {
                            }
                        }
                    } else {
                        setStatus("IMLEC: SANAL (SHIZUKU ONAYI YOK)")
                    }
                }
            }
        } catch (_: Exception) {
        }
    }

    fun sockPath(): String {
        return try {
            AppGraph.appContext.filesDir.resolve("nbpointer.sock").absolutePath
        } catch (_: Exception) {
            "/data/local/tmp/nbpointer.sock"
        }
    }

    fun hasBackend(): Boolean {
        synchronized(lock) {
            return backend != null
        }
    }

    fun setMode(next: PointerMode) {
        mode.value = next
        val active = synchronized(lock) { sessionOn }
        if (!active) {
            return
        }
        if (next == PointerMode.VIRTUAL) {
            sessionStop()
        } else {
            try {
                Thread { sessionStart() }.start()
            } catch (_: Exception) {
                sessionStart()
            }
        }
    }

    fun sessionStart() {
        val hadBackend = synchronized(lock) {
            sessionOn = true
            backend != null
        }
        Log.d("northbridge", "pointer session enter mode=" + mode.value + " backend=" + hadBackend)
        if (hadBackend) {
            if (pingBackend()) {
                Log.d("northbridge", "pointer session keep backend alive")
                return
            }
            Log.d("northbridge", "pointer session backend stale")
        }
        if (!claimStart()) {
            Log.d("northbridge", "pointer session busy")
            return
        }
        try {
            val raced = synchronized(lock) { backend != null }
            if (raced) {
                Log.d("northbridge", "pointer session joined backend")
                return
            }
            if (mode.value == PointerMode.VIRTUAL) {
                Log.d("northbridge", "pointer session virtual mode")
                setStatus("IMLEC: SANAL")
                return
            }
            val rootOk = startRoot()
            Log.d("northbridge", "pointer session root=" + rootOk)
            if (rootOk) {
                return
            }
            if (mode.value == PointerMode.REAL) {
                maybeAskShizuku()
                val shizukuOk = startShizuku()
                Log.d("northbridge", "pointer session shizuku=" + shizukuOk)
                if (shizukuOk) {
                    return
                }
                setStatus(shizukuStatus())
                return
            }
            val shizukuOk = startShizuku()
            Log.d("northbridge", "pointer session shizuku=" + shizukuOk)
            if (shizukuOk) {
                return
            }
            if (!shizukuAlive()) {
                Log.d("northbridge", "pointer session no root no shizuku")
                setStatus("IMLEC: SANAL (KOK YOK)")
            } else {
                Log.d("northbridge", "pointer session shizuku denied")
                setStatus(shizukuStatus())
            }
        } finally {
            synchronized(lock) { starting = false }
        }
    }

    fun sessionStop() {
        val link = synchronized(lock) {
            sessionOn = false
            held.clear()
            posInit = false
            accX = 0.0
            accY = 0.0
            freeInit = false
            geomW = 0
            geomH = 0
            val old = backend
            backend = null
            old
        }
        try {
            link?.close()
        } catch (_: Exception) {
        }
        unbindShizuku()
        setStatus("IMLEC: SANAL")
    }

    fun letterbox(nx: Double, ny: Double, w: Int, h: Int): Pair<Double, Double> {
        val aspect = PC_W / PC_H
        val areaW: Double
        val areaH: Double
        val offX: Double
        val offY: Double
        if (w.toDouble() / h.toDouble() >= aspect) {
            areaH = h.toDouble()
            areaW = areaH * aspect
            offX = (w.toDouble() - areaW) / 2.0
            offY = 0.0
        } else {
            areaW = w.toDouble()
            areaH = areaW / aspect
            offX = 0.0
            offY = h.toDouble() - areaH
        }
        return (offX + nx * areaW) to (offY + ny * areaH)
    }

    fun moveMirror(nx: Double, ny: Double, w: Int, h: Int) {
        val link = synchronized(lock) { backend }
        if (link == null) {
            dropLog("mirror dropped (no backend)")
            return
        }
        if (w <= 0 || h <= 0) {
            return
        }
        val cx = nx.coerceIn(0.0, 1.0)
        val cy = ny.coerceIn(0.0, 1.0)
        synchronized(lock) {
            if (backend == null) {
                return
            }
            val t = letterbox(cx, cy, w, h)
            fx = t.first
            fy = t.second
            geomW = w
            geomH = h
            freeInit = true
        }
        flush(link)
    }

    fun moveFree(dxPx: Double, dyPx: Double, w: Int, h: Int) {
        if (dxPx == 0.0 && dyPx == 0.0) {
            return
        }
        val link = synchronized(lock) { backend }
        if (link == null) {
            dropLog("free dropped (no backend)")
            return
        }
        if (w <= 0 || h <= 0) {
            return
        }
        synchronized(lock) {
            if (backend == null) {
                return
            }
            if (!freeInit) {
                fx = w.toDouble() / 2.0
                fy = h.toDouble() / 2.0
                geomW = w
                geomH = h
                freeInit = true
            } else if (w != geomW || h != geomH) {
                if (geomW > 0 && geomH > 0) {
                    fx = fx * w.toDouble() / geomW.toDouble()
                    fy = fy * h.toDouble() / geomH.toDouble()
                }
                geomW = w
                geomH = h
            }
            fx = (fx + dxPx * MIRROR_FREE_GAIN).coerceIn(0.0, w.toDouble())
            fy = (fy + dyPx * MIRROR_FREE_GAIN).coerceIn(0.0, h.toDouble())
        }
        flush(link)
    }

    fun freePos(): Pair<Int, Int> {
        synchronized(lock) {
            return ex to ey
        }
    }

    fun curNorm(): Pair<Double, Double> {
        synchronized(lock) {
            if (freeInit && geomW > 0 && geomH > 0) {
                return (fx / geomW.toDouble()).coerceIn(0.0, 1.0) to (fy / geomH.toDouble()).coerceIn(0.0, 1.0)
            }
            return 0.5 to 0.5
        }
    }

    private fun flush(link: DaemonLink) {
        val dx: Int
        val dy: Int
        synchronized(lock) {
            dx = fx.toInt() - ex
            dy = fy.toInt() - ey
            if (dx == 0 && dy == 0) {
                return
            }
            ex += dx
            ey += dy
        }
        if (!link.write("m " + dx + " " + dy)) {
            drop(link)
        }
    }

    private var moveLogCount = 0

    fun moveRel(dxPx: Double, dyPx: Double, w: Int, h: Int) {
        if (dxPx == 0.0 && dyPx == 0.0) {
            return
        }
        if (w <= 0 || h <= 0) {
            return
        }
        val link = synchronized(lock) { backend } ?: run {
            dropLog("moverel dropped (no backend)")
            return
        }
        var dx = 0
        var dy = 0
        synchronized(lock) {
            if (backend == null) {
                return
            }
            accX += dxPx * POINTER_SENSITIVITY
            accY += dyPx * POINTER_SENSITIVITY
            dx = accX.toInt()
            dy = accY.toInt()
            accX -= dx
            accY -= dy
            dx = compressDelta(dx)
            dy = compressDelta(dy)
            val bound = maxOf(w, h)
            dx = dx.coerceIn(-bound, bound)
            dy = dy.coerceIn(-bound, bound)
            if (posInit) {
                posX += dx.toDouble() / w
                posY += dy.toDouble() / h
            }
        }
        if (dx != 0 || dy != 0) {
            moveLogCount++
            if (moveLogCount <= 10 || moveLogCount % 100 == 0) {
                Log.w("northbridge", "pointer move $dx,$dy #$moveLogCount")
            }
            if (!link.write("m " + dx + " " + dy)) {
                drop(link)
            }
        }
    }

    fun button(code: Int, down: Boolean) {
        val link = synchronized(lock) { backend } ?: return
        synchronized(lock) {
            if (down) {
                held.add(code)
            } else {
                held.remove(code)
            }
        }
        if (!link.write("b " + code + " " + if (down) "1" else "0")) {
            drop(link)
        }
    }

    fun releaseAll() {
        val codes = synchronized(lock) {
            val snapshot = held.toList()
            held.clear()
            snapshot
        }
        if (codes.isEmpty()) {
            return
        }
        val link = synchronized(lock) { backend } ?: return
        for (code in codes) {
            link.write("b " + code + " 0")
        }
    }

    fun wheel(delta: Int) {
        if (delta == 0) {
            return
        }
        val link = synchronized(lock) { backend } ?: return
        if (!link.write("w " + delta)) {
            drop(link)
        }
    }

    private fun setStatus(text: String) {
        _status.value = text
        Log.d("northbridge", "pointer: $text")
    }

    private var lastDropLogAt = 0L

    private fun dropLog(reason: String) {
        val now = System.currentTimeMillis()
        if (now - lastDropLogAt > 1000) {
            lastDropLogAt = now
            Log.w("northbridge", "pointer $reason")
        }
    }

    private fun startRoot(): Boolean {
        val rooted = rootAvailable()
        Log.d("northbridge", "pointer root avail=" + rooted)
        if (!rooted) {
            return false
        }
        killStaleDaemons()
        dropStaleSock()
        val context = try {
            AppGraph.appContext
        } catch (_: Exception) {
            null
        }
        if (context == null) {
            Log.w("northbridge", "pointer root no context")
            return false
        }
        val apk = context.applicationInfo.sourceDir
        val lib = PointerJni.extractLib(apk, context.filesDir, PRIV_LIB)
        Log.d("northbridge", "pointer root lib=" + (lib != null))
        if (lib == null) {
            return false
        }
        val entry = PointerDaemon::class.java.name
        val shell = "CLASSPATH=" + apk + " app_process /system/bin " + entry + " " + sockPath() + " " + lib
        val proc = try {
            ProcessBuilder("su", "-c", shell).start()
        } catch (_: Exception) {
            null
        }
        Log.d("northbridge", "pointer root proc=" + (proc != null))
        if (proc == null) {
            return false
        }
        val link = waitSocket(proc)
        Log.d("northbridge", "pointer root link=" + (link != null))
        if (link == null) {
            return false
        }
        attach(link, "IMLEC: GERCEK (KOK)")
        return true
    }

    private fun startShizuku(): Boolean {
        if (!shizukuAlive() || !shizukuGranted()) {
            return false
        }
        val context = try {
            AppGraph.appContext
        } catch (_: Exception) {
            null
        } ?: return false
        val apk = context.applicationInfo.sourceDir
        PointerJni.extractLib(apk, File("/data/local/tmp"), "nbpointer_app.so")
        dropStaleSock()
        val args = Shizuku.UserServiceArgs(
            ComponentName(context.packageName, PointerUserService::class.java.name),
        ).daemon(false).processNameSuffix("pointer").version(appVersion(context)).tag("nbpointer")
        val latched = CountDownLatch(1)
        val conn = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
                latched.countDown()
            }

            override fun onServiceDisconnected(name: ComponentName?) {
            }
        }
        try {
            Shizuku.bindUserService(args, conn)
        } catch (_: Exception) {
            return false
        }
        try {
            latched.await(5, TimeUnit.SECONDS)
        } catch (_: Exception) {
        }
        val link = waitSocket(null)
        if (link == null) {
            try {
                Shizuku.unbindUserService(args, conn, true)
            } catch (_: Exception) {
            }
            return false
        }
        synchronized(lock) {
            shizukuArgs = args
            shizukuConn = conn
        }
        attach(link, "IMLEC: GERCEK (SHIZUKU)")
        return true
    }

    private fun unbindShizuku() {
        val pair = synchronized(lock) {
            val saved = shizukuArgs to shizukuConn
            shizukuArgs = null
            shizukuConn = null
            saved
        }
        val args = pair.first
        val conn = pair.second
        if (args != null && conn != null) {
            try {
                Shizuku.unbindUserService(args, conn, true)
            } catch (_: Exception) {
            }
        }
    }

    private fun attach(link: DaemonLink, label: String) {
        val old = synchronized(lock) {
            val previous = backend
            backend = link
            held.clear()
            posInit = false
            posX = 0.0
            posY = 0.0
            accX = 0.0
            accY = 0.0
            previous
        }
        try {
            old?.close()
        } catch (_: Exception) {
        }
        link.write("m -4096 -4096")
        val homeOk = link.write("m -4096 -4096")
        Log.d("northbridge", "pointer attach " + label + " home=" + homeOk)
        synchronized(lock) {
            posInit = false
            accX = 0.0
            accY = 0.0
            freeInit = false
            geomW = 0
            geomH = 0
        }
        val dm = try {
            AppGraph.appContext.resources.displayMetrics
        } catch (_: Exception) {
            null
        }
        if (dm != null && dm.widthPixels > 0 && dm.heightPixels > 0) {
            val cx = dm.widthPixels / 2
            val cy = dm.heightPixels / 2
            if (link.write("m " + cx + " " + cy)) {
                synchronized(lock) {
                    fx = cx.toDouble()
                    fy = cy.toDouble()
                    ex = cx
                    ey = cy
                    geomW = dm.widthPixels
                    geomH = dm.heightPixels
                    freeInit = true
                }
            }
        }
        setStatus(label)
    }

    private fun drop(link: DaemonLink) {
        val dead = synchronized(lock) {
            if (backend === link) {
                backend = null
                held.clear()
                posInit = false
                freeInit = false
                link
            } else {
                null
            }
        }
        try {
            dead?.close()
        } catch (_: Exception) {
        }
        if (dead != null) {
            Log.w("northbridge", "pointer drop backend cleared")
            unbindShizuku()
            setStatus("IMLEC: SANAL (BAGLANTI KOPTU)")
        }
    }

    private fun waitSocket(proc: Process?): DaemonLink? {
        val start = SystemClock.uptimeMillis()
        val deadline = start + DAEMON_WAIT_MS
        while (SystemClock.uptimeMillis() < deadline) {
            if (proc != null) {
                try {
                    proc.exitValue()
                    closeQuiet(proc)
                    Log.w("northbridge", "pointer wait proc died")
                    return null
                } catch (_: IllegalThreadStateException) {
                }
            }
            try {
                val socket = LocalSocket()
                socket.connect(
                    LocalSocketAddress(sockPath(), LocalSocketAddress.Namespace.FILESYSTEM),
                )
                Log.d("northbridge", "pointer wait ok ms=" + (SystemClock.uptimeMillis() - start))
                return DaemonLink(proc, socket, socket.outputStream.bufferedWriter())
            } catch (_: Exception) {
            }
            try {
                Thread.sleep(150)
            } catch (_: Exception) {
            }
        }
        if (proc != null) {
            closeQuiet(proc)
        }
        killStaleDaemons()
        Log.w("northbridge", "pointer wait timeout")
        return null
    }

    private fun rootAvailable(): Boolean {
        return try {
            ProcessBuilder("su", "-c", "exit 0").start().waitFor() == 0
        } catch (_: Exception) {
            false
        }
    }

    private fun shizukuAlive(): Boolean {
        return try {
            Shizuku.pingBinder()
        } catch (_: Exception) {
            false
        }
    }

    private fun shizukuGranted(): Boolean {
        return try {
            shizukuAlive() && Shizuku.checkSelfPermission() == PackageManager.PERMISSION_GRANTED
        } catch (_: Exception) {
            false
        }
    }

    private fun shizukuStatus(): String {
        if (!shizukuAlive()) {
            return "IMLEC: SANAL (SHIZUKU YOK)"
        }
        if (!shizukuGranted()) {
            return "IMLEC: SANAL (SHIZUKU ONAYI YOK)"
        }
        return "IMLEC: SANAL (SHIZUKU ERISIM YOK)"
    }

    private fun maybeAskShizuku() {
        val ask = synchronized(lock) {
            if (shizukuAsked) {
                false
            } else {
                shizukuAsked = true
                true
            }
        }
        if (!ask) {
            return
        }
        try {
            if (shizukuAlive() && !shizukuGranted()) {
                Shizuku.requestPermission(SHIZUKU_REQ)
            }
        } catch (_: Exception) {
        }
    }

    private fun closeQuiet(proc: Process) {        try {
            proc.destroy()
        } catch (_: Exception) {
        }
    }

    private fun compressDelta(v: Int): Int {
        val a = kotlin.math.abs(v)
        if (a <= FLICK_LINEAR_PX) {
            return v
        }
        val tamed = FLICK_LINEAR_PX + ((a - FLICK_LINEAR_PX) * FLICK_EXCESS_GAIN).toInt()
        return if (v > 0) tamed else -tamed
    }

    private fun pingBackend(): Boolean {
        val link = synchronized(lock) { backend } ?: return false
        if (!link.ping()) {
            drop(link)
            return false
        }
        return true
    }

    private var starting = false

    private fun claimStart(): Boolean {
        val deadline = SystemClock.uptimeMillis() + DAEMON_WAIT_MS
        while (true) {
            synchronized(lock) {
                if (!starting) {
                    starting = true
                    return true
                }
            }
            if (SystemClock.uptimeMillis() >= deadline) {
                return false
            }
            try {
                Thread.sleep(200)
            } catch (_: Exception) {
            }
        }
    }

    private fun killStaleDaemons() {
        try {
            ProcessBuilder("su", "-c", "pkill -f [P]ointerDaemon").start().waitFor()
        } catch (_: Exception) {
        }
    }

    private fun dropStaleSock() {
        val path = sockPath()
        try {
            File(path).delete()
        } catch (_: Exception) {
        }
        try {
            ProcessBuilder("su", "-c", "rm -f " + path).start().waitFor()
        } catch (_: Exception) {
        }
    }

    private fun appVersion(context: android.content.Context): Int {
        return try {
            val info = context.packageManager.getPackageInfo(context.packageName, 0)
            PackageInfoCompat.getLongVersionCode(info).toInt()
        } catch (_: Exception) {
            1
        }
    }

    private class DaemonLink(
        val proc: Process?,
        val socket: LocalSocket,
        val out: java.io.BufferedWriter,
    ) {
        fun ping(): Boolean {
            return write("p")
        }

        fun write(line: String): Boolean {
            return try {
                out.write(line)
                out.newLine()
                out.flush()
                true
            } catch (_: Exception) {
                false
            }
        }

        fun close() {
            try {
                write("q")
            } catch (_: Exception) {
            }
            try {
                socket.close()
            } catch (_: Exception) {
            }
            try {
                proc?.destroy()
            } catch (_: Exception) {
            }
        }
    }
}
