package bit.kedybaykedy.northbridge.input

import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.IBinder

class PointerUserService : Service() {

    override fun onBind(intent: Intent?): IBinder {
        return Binder()
    }

    override fun onCreate() {
        super.onCreate()
        try {
            Thread({
                if (PointerJni.loadExplicit(NativePointer.TMP_LIB)) {
                    PointerDaemon.serve(NativePointer.SOCK_PATH)
                }
            }, "nbpointerd").start()
        } catch (_: Exception) {
        }
    }
}
