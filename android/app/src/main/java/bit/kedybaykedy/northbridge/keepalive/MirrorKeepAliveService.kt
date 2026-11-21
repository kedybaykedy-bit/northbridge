package bit.kedybaykedy.northbridge.keepalive

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder
import bit.kedybaykedy.northbridge.R

class MirrorKeepAliveService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel("mirror", "Ayna", NotificationManager.IMPORTANCE_LOW),
            )
        }
        val notification = Notification.Builder(this, "mirror")
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("Northbridge")
            .setContentText("Ayna bağlantısı arka planda açık")
            .setOngoing(true)
            .build()
        startForeground(1, notification)
        return START_STICKY
    }
}
