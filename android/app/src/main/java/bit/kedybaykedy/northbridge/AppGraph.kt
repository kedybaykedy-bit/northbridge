package bit.kedybaykedy.northbridge

import android.app.Application
import android.content.Context
import android.content.SharedPreferences
import bit.kedybaykedy.northbridge.data.DeviceStore
import bit.kedybaykedy.northbridge.transport.IdentityStore
import bit.kedybaykedy.northbridge.transport.InputChannel
import bit.kedybaykedy.northbridge.transport.PhoneIdentity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

object AppGraph {

    private lateinit var context: Context
    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    val appContext: Context
        get() = context

    val prefs: SharedPreferences
        get() = context.getSharedPreferences("northbridge", Context.MODE_PRIVATE)

    val identity: PhoneIdentity by lazy { IdentityStore.loadOrGenerate(prefs) }

    val deviceStore: DeviceStore by lazy { DeviceStore(prefs) }

    val inputChannel: InputChannel by lazy {
        InputChannel(identity, CoroutineScope(SupervisorJob() + Dispatchers.IO))
    }

    fun init(application: Application) {
        if (::context.isInitialized) return
        context = application.applicationContext
        appScope.launch { identity.fingerprint }
        appScope.launch {
            combine(deviceStore.devices, deviceStore.activeFingerprint) { devices, fingerprint ->
                devices.find { it.fingerprint == fingerprint }
            }.collect { active -> inputChannel.attach(active) }
        }
    }
}
