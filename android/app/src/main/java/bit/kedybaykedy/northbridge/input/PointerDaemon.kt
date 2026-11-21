package bit.kedybaykedy.northbridge.input

import android.net.LocalServerSocket
import android.net.LocalSocket
import android.net.LocalSocketAddress
import android.util.Log
import java.io.File

object PointerDaemon {
    private const val TAG = "nbpointerd"

    @JvmStatic
    fun main(args: Array<String>) {
        if (args.isNotEmpty() && args[0] == "probe") {
            probe(args.drop(1))
            return
        }
        if (args.isNotEmpty() && args[0] == "selftest") {
            selftest(args.drop(1))
            return
        }
        if (args.isEmpty()) {
            return
        }
        val lib = args.getOrNull(1)
        if (lib != null && !PointerJni.loadExplicit(lib)) {
            Log.e(TAG, "load " + lib)
            return
        }
        serve(args[0])
    }

    fun serve(path: String) {
        try {
            File(path).delete()
        } catch (_: Exception) {
        }
        val rc = PointerJni.nbCreate()
        if (rc != 0) {
            Log.e(TAG, "nbCreate rc=" + rc)
            return
        }
        var binderSock: LocalSocket? = null
        var server: LocalServerSocket? = null
        try {
            binderSock = LocalSocket()
            binderSock.bind(LocalSocketAddress(path, LocalSocketAddress.Namespace.FILESYSTEM))
            server = LocalServerSocket(binderSock.fileDescriptor)
            val client = server.accept()
            try {
                val input = client.inputStream.bufferedReader()
                while (true) {
                    val line = input.readLine() ?: break
                    if (line == "q") {
                        break
                    }
                    handle(line)
                }
            } catch (_: Exception) {
            } finally {
                try {
                    client.close()
                } catch (_: Exception) {
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "serve " + e.message)
        } finally {
            try {
                server?.close()
            } catch (_: Exception) {
            }
            try {
                binderSock?.close()
            } catch (_: Exception) {
            }
            PointerJni.nbDestroy()
            try {
                File(path).delete()
            } catch (_: Exception) {
            }
        }
    }

    private fun handle(line: String) {
        val parts = line.split(' ')
        if (parts.isEmpty()) {
            return
        }
        when (parts[0]) {
            "m" -> {
                if (parts.size >= 3) {
                    val dx = parts[1].toIntOrNull()
                    val dy = parts[2].toIntOrNull()
                    if (dx != null && dy != null) {
                        PointerJni.nbMoveRel(dx, dy)
                    }
                }
            }
            "b" -> {
                if (parts.size >= 3) {
                    val code = parts[1].toIntOrNull()
                    val down = parts[2] == "1"
                    if (code != null) {
                        PointerJni.nbButton(code, down)
                    }
                }
            }
            "w" -> {
                if (parts.size >= 2) {
                    val delta = parts[1].toIntOrNull()
                    if (delta != null) {
                        PointerJni.nbWheel(delta)
                    }
                }
            }
        }
    }

    private fun probe(args: List<String>) {
        if (args.isEmpty()) {
            return
        }
        val path = args[0]
        try {
            val socket = LocalSocket()
            socket.connect(
                LocalSocketAddress(path, LocalSocketAddress.Namespace.FILESYSTEM),
            )
            val out = socket.outputStream.bufferedWriter()
            for (line in args.drop(1)) {
                out.write(line)
                out.newLine()
            }
            out.write("q")
            out.newLine()
            out.flush()
            socket.close()
        } catch (e: Exception) {
            Log.e(TAG, "probe " + e.message)
        }
    }

    private fun selftest(args: List<String>) {
        if (args.size >= 2) {
            PointerJni.extractLib(args[0], File(args[1]), "nbpointer_selftest.so")
        }
        if (!PointerJni.loadExplicit("/data/local/tmp/nbpointer_selftest.so")) {
            println("nbpointerd selftest load failed")
            return
        }
        val rc = PointerJni.nbCreate()
        println("nbpointerd selftest create rc=" + rc)
        if (rc != 0) {
            return
        }
        try {
            val inputs = try {
                File("/dev/input").list()?.sorted()
            } catch (_: Exception) {
                null
            }
            println("nbpointerd inputs=" + inputs)
            repeat(30) {
                PointerJni.nbMoveRel(15, 8)
                Thread.sleep(40)
            }
            PointerJni.nbButton(272, true)
            Thread.sleep(120)
            PointerJni.nbButton(272, false)
            PointerJni.nbWheel(3)
            PointerJni.nbWheel(-3)
            println("nbpointerd selftest events ok")
            Thread.sleep(4000)
        } catch (e: Exception) {
            println("nbpointerd selftest " + e.message)
        } finally {
            PointerJni.nbDestroy()
            println("nbpointerd selftest done")
        }
    }
}
