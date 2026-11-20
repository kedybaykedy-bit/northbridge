package bit.kedybaykedy.northbridge

import android.app.Application

class NorthbridgeApp : Application() {

    override fun onCreate() {
        super.onCreate()
        AppGraph.init(this)
    }
}
