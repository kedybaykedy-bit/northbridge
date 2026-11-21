package bit.kedybaykedy.northbridge.keepalive

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder
import bit.kedybaykedy.northbridge.R
import bit.kedybaykedy.northbridge.transport.FileServer
import bit.kedybaykedy.northbridge.transport.PacketServer

class PacketKeepAliveService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel("packet", "Bağlantı", NotificationManager.IMPORTANCE_LOW),
            )
        }
        val notification = Notification.Builder(this, "packet")
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("Northbridge")
            .setContentText("Gelen bağlantılar için arka planda açık")
            .setOngoing(true)
            .build()
        startForeground(2, notification)
        PacketServer.start()
        FileServer.start()
        return START_STICKY
    }
}
