package bit.kedybaykedy.northbridge.ui

import androidx.lifecycle.ViewModel
import bit.kedybaykedy.northbridge.AppGraph
import bit.kedybaykedy.northbridge.data.TrustedDevice
import kotlinx.coroutines.flow.StateFlow

class DevicesViewModel : ViewModel() {

    val devices: StateFlow<List<TrustedDevice>> = AppGraph.deviceStore.devices
    val activeFingerprint: StateFlow<String?> = AppGraph.deviceStore.activeFingerprint

    fun setActive(device: TrustedDevice) {
        AppGraph.deviceStore.setActive(device.fingerprint)
    }

    fun unpair(device: TrustedDevice) {
        AppGraph.deviceStore.remove(device.fingerprint)
    }
}
