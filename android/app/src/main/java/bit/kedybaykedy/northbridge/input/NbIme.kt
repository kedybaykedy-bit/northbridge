package bit.kedybaykedy.northbridge.input

import android.inputmethodservice.InputMethodService
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection

class NbIme : InputMethodService() {

    override fun onCreateInputView(): View {
        return View(this).apply {
            setBackgroundColor(-16777216)
        }
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        active = this
    }

    override fun onFinishInput() {
        if (active === this) {
            active = null
        }
        super.onFinishInput()
    }

    companion object {
        private const val COMPONENT = "bit.kedybaykedy.northbridge/.input.NbIme"
        private const val SETTLE_MS = 400L
        private const val READY_POLL_MS = 150L
        private const val READY_TRIES = 10
        private const val COMMIT_TIMEOUT_S = 2L
        private const val RESTORE_IDLE_MS = 600L

        @Volatile
        private var active: NbIme? = null

        @Volatile
        private var previousIme: String? = null

        private val restoreExecutor = java.util.concurrent.Executors.newSingleThreadScheduledExecutor { task ->
            Thread(task).apply { isDaemon = true }
        }

        @Volatile
        private var restoreFuture: java.util.concurrent.ScheduledFuture<*>? = null

        fun isReady(): Boolean {
            return try {
                active?.currentInputConnection != null
            } catch (_: Exception) {
                false
            }
        }

        fun commit(text: String): Boolean {
            if (text.isEmpty()) {
                return false
            }
            if (!ensureActive()) {
                return false
            }
            var tries = 0
            while (!isReady() && tries < READY_TRIES) {
                try {
                    Thread.sleep(READY_POLL_MS)
                } catch (_: Exception) {
                }
                tries++
            }
            val ok = commitNow(text)
            scheduleRestore()
            return ok
        }

        fun ensureActive(): Boolean {
            return try {
                val current = shell("settings get secure default_input_method").trim()
                if (current != COMPONENT && current.isNotEmpty() && current != "null") {
                    previousIme = current
                }
                if (!shell("ime list -s").contains(COMPONENT)) {
                    shell("ime enable " + COMPONENT)
                }
                if (current != COMPONENT) {
                    shell("ime set " + COMPONENT)
                    try {
                        Thread.sleep(SETTLE_MS)
                    } catch (_: Exception) {
                    }
                }
                true
            } catch (_: Exception) {
                false
            }
        }

        fun restorePrevious(): Boolean {
            val prev = previousIme ?: return true
            previousIme = null
            return try {
                shell("ime set " + prev)
                true
            } catch (_: Exception) {
                false
            }
        }

        private fun commitNow(text: String): Boolean {
            val ime = active ?: return false
            if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) {
                return commitDirect(ime, text)
            }
            var result = false
            val done = java.util.concurrent.CountDownLatch(1)
            try {
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    try {
                        result = commitDirect(ime, text)
                    } catch (_: Exception) {
                    } finally {
                        done.countDown()
                    }
                }
            } catch (_: Exception) {
                done.countDown()
            }
            try {
                done.await(COMMIT_TIMEOUT_S, java.util.concurrent.TimeUnit.SECONDS)
            } catch (_: Exception) {
            }
            return result
        }

        private fun commitDirect(ime: NbIme, text: String): Boolean {
            return try {
                val ic: InputConnection? = ime.currentInputConnection
                if (ic == null) {
                    false
                } else if (!ic.commitText(text, 1)) {
                    false
                } else {
                    ic.finishComposingText()
                    true
                }
            } catch (_: Exception) {
                false
            }
        }

        private fun scheduleRestore() {
            try {
                restoreFuture?.cancel(false)
                restoreFuture = restoreExecutor.schedule(
                    { restorePrevious() },
                    RESTORE_IDLE_MS,
                    java.util.concurrent.TimeUnit.MILLISECONDS,
                )
            } catch (_: Exception) {
            }
        }

        private fun shell(cmd: String): String {
            return try {
                val proc = ProcessBuilder("su", "-c", cmd).start()
                val out = proc.inputStream.bufferedReader().readText()
                proc.waitFor()
                out
            } catch (_: Exception) {
                ""
            }
        }
    }
}
