package com.forgata.btclip

import android.Manifest
import android.os.Build
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat

class MainActivity : AppCompatActivity() {
    companion object {
        private const val REQUEST_PERMISSIONS = 100
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val runtimePermissions = mutableListOf<String>()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            runtimePermissions += Manifest.permission.BLUETOOTH_CONNECT
            runtimePermissions += Manifest.permission.BLUETOOTH_SCAN
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            runtimePermissions += Manifest.permission.POST_NOTIFICATIONS
        }

        if (runtimePermissions.isNotEmpty()) {
            ActivityCompat.requestPermissions(
                this,
                runtimePermissions.toTypedArray(),
                REQUEST_PERMISSIONS
            )
        }
    }
}
