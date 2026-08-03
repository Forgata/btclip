package com.forgata.btclip

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.bluetooth.BluetoothAdapter
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.FileProvider
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.util.concurrent.Executors

class BluetoothClipboardService : Service() {
    companion object {
        private const val TAG = "BTClipboardService"
        private const val CHANNEL_ID = "btclip_foreground"
        private const val NOTIFICATION_ID = 42
    }

    private val executor = Executors.newSingleThreadExecutor()
    private var client: BluetoothClipboardClient? = null
    private lateinit var clipboardManager: ClipboardManager
    private var lastSentText: String? = null
    private var lastSentImageHash: Int = 0
    @Volatile private var isInternalWrite = false

    private val clipboardListener = ClipboardManager.OnPrimaryClipChangedListener {
        executor.execute {
            if (isInternalWrite) {
                isInternalWrite = false
                return@execute
            }
            handleClipboardChanged()
        }
    }

    override fun onCreate() {
        super.onCreate()
        clipboardManager = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        clipboardManager.addPrimaryClipChangedListener(clipboardListener)
    }

    private fun onHostTextReceived(text: String) {
        isInternalWrite = true
        val clip = ClipData.newPlainText("BTClip", text)
        clipboardManager.setPrimaryClip(clip)
        Log.i(TAG, "Updated phone clipboard with host text")
    }

    private fun onHostImageReceived(imageBytes: ByteArray) {
        try {
            val cachedFile = File(cacheDir, "btclip_received_image.png")
            cachedFile.writeBytes(imageBytes)
            val uri: Uri = FileProvider.getUriForFile(
                this,
                "$packageName.fileprovider",
                cachedFile
            )
            val clip = ClipData.newUri(contentResolver, "BTClip Image", uri)
            isInternalWrite = true
            clipboardManager.setPrimaryClip(clip)
            Log.i(TAG, "Updated phone clipboard with host image")
        } catch (e: IOException) {
            Log.e(TAG, "Failed to update phone clipboard with received image", e)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, createNotification())
        executor.execute { connectToFirstPairedDevice() }
        return START_STICKY
    }

    override fun onDestroy() {
        clipboardManager.removePrimaryClipChangedListener(clipboardListener)
        client?.disconnect()
        executor.shutdownNow()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotification(): Notification {
        createNotificationChannel()

        val activityIntent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }

        val pendingIntent = PendingIntent.getActivity(
            this,
            0,
            activityIntent,
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                PendingIntent.FLAG_IMMUTABLE
            } else {
                0
            }
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.notification_title))
            .setContentText(getString(R.string.notification_text))
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .build()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notification_channel_name),
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = getString(R.string.notification_channel_description)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(channel)
        }
    }

    private fun connectToFirstPairedDevice() {
        val adapter = BluetoothAdapter.getDefaultAdapter()
        if (adapter == null) {
            Log.e(TAG, "No Bluetooth adapter available")
            return
        }

        val bonded = adapter.bondedDevices
        if (bonded.isEmpty()) {
            Log.w(TAG, "No paired Bluetooth devices found")
            return
        }

        val device = bonded.find {
            val name = it.name ?: ""
            name.contains("pc", ignoreCase = true) || name.contains("desktop", ignoreCase = true)
        } ?: bonded.first()

        Log.i(TAG, "Connecting to paired device ${device.name} (${device.address})")

        val client = BluetoothClipboardClient(device, object : BluetoothClipboardClient.Listener {
            override fun onTextReceived(text: String) {
                onHostTextReceived(text)
            }

            override fun onImageReceived(imageBytes: ByteArray) {
                onHostImageReceived(imageBytes)
            }
        })
        if (client.connect()) {
            this.client = client
            Log.i(TAG, "Bluetooth clipboard sync connected")
            handleClipboardChanged()
        } else {
            Log.e(TAG, "Bluetooth clipboard sync failed to connect")
        }
    }

    private fun handleClipboardChanged() {
        val client = client
        if (client == null || !client.isConnected()) {
            Log.w(TAG, "Clipboard changed but client is not connected")
            return
        }

        Log.i(TAG, "Clipboard changed; client connected=${client.isConnected()}")
        val clip = clipboardManager.primaryClip
        if (clip == null || clip.itemCount == 0) {
            return
        }

        val item = clip.getItemAt(0)
        val text = item.coerceToText(this)?.toString()?.trim()
        if (!text.isNullOrEmpty() && text != lastSentText) {
            Log.i(TAG, "Sending clipboard text to host: $text")
            client.sendText(text)
            lastSentText = text
            return
        }

        val uri = item.uri
        if (uri != null) {
            try {
                contentResolver.openInputStream(uri)?.use { inputStream ->
                    val imageBytes = inputStream.readBytes()
                    val bitmap = BitmapFactory.decodeByteArray(imageBytes, 0, imageBytes.size)
                    if (bitmap != null) {
                        val pngBytes = ByteArrayOutputStream().use {
                            bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
                            it.toByteArray()
                        }

                        val imageHash = pngBytes.contentHashCode()
                        if (imageHash != lastSentImageHash) {
                            Log.i(TAG, "Sending clipboard image to host (${pngBytes.size} bytes)")
                            client.sendImage(pngBytes)
                            lastSentImageHash = imageHash
                        }
                    }
                }
            } catch (e: IOException) {
                Log.e(TAG, "Failed to read image URI from clipboard", e)
            }
        }

        val intent = item.intent
        if (intent != null) {
            val uriFromIntent = intent.data
            if (uriFromIntent != null) {
                try {
                    contentResolver.openInputStream(uriFromIntent)?.use { inputStream ->
                        val imageBytes = inputStream.readBytes()
                        val bitmap = BitmapFactory.decodeByteArray(imageBytes, 0, imageBytes.size)
                        if (bitmap != null) {
                            val pngBytes = ByteArrayOutputStream().use {
                                bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
                                it.toByteArray()
                            }

                            val imageHash = pngBytes.contentHashCode()
                            if (imageHash != lastSentImageHash) {
                                Log.i(TAG, "Sending clipboard image from intent to host (${pngBytes.size} bytes)")
                                client.sendImage(pngBytes)
                                lastSentImageHash = imageHash
                            }
                        }
                    }
                } catch (e: IOException) {
                    Log.e(TAG, "Failed to read image URI from clipboard intent", e)
                }
            }
        }
    }
}
