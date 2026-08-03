package com.forgata.btclip

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import android.util.Log
import java.io.InputStream
import java.io.OutputStream
import java.io.IOException
import java.util.UUID
import kotlin.concurrent.thread

class BluetoothClipboardClient(
    private val device: BluetoothDevice,
    private val listener: Listener
) {
    interface Listener {
        fun onTextReceived(text: String)
        fun onImageReceived(imageBytes: ByteArray)
    }

    companion object {
        private const val TAG = "BTClipboardClient"
        private val SPP_UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")
    }

    private var socket: BluetoothSocket? = null
    private var outputStream: OutputStream? = null
    private var inputStream: InputStream? = null
    @Volatile private var connected = false
    private var readThread: Thread? = null

    fun connect(): Boolean {
        return try {
            connectSocket(device.createRfcommSocketToServiceRecord(SPP_UUID))
        } catch (e: IOException) {
            Log.w(TAG, "Secure RFCOMM failed, retrying insecure socket", e)
            try {
                connectSocket(device.createInsecureRfcommSocketToServiceRecord(SPP_UUID))
            } catch (inner: IOException) {
                Log.e(TAG, "Failed to connect with insecure RFCOMM", inner)
                disconnect()
                false
            }
        }
    }

    @Throws(IOException::class)
    private fun connectSocket(socket: BluetoothSocket): Boolean {
        socket.connect()
        this.socket = socket
        this.outputStream = socket.outputStream
        this.inputStream = socket.inputStream
        connected = true
        startReadLoop()
        Log.i(TAG, "Connected to device ${device.name} (${device.address})")
        return true
    }

    fun disconnect() {
        connected = false
        readThread?.interrupt()
        readThread = null
        try {
            inputStream?.close()
        } catch (_: IOException) {
        }
        try {
            outputStream?.close()
        } catch (_: IOException) {
        }
        try {
            socket?.close()
        } catch (_: IOException) {
        }
        socket = null
        inputStream = null
        outputStream = null
    }

    fun isConnected(): Boolean = connected

    fun sendText(text: String) {
        sendFrame(ProtocolFrame.TYPE_TEXT, text.toByteArray(Charsets.UTF_8))
    }

    fun sendImage(imageBytes: ByteArray) {
        sendFrame(ProtocolFrame.TYPE_IMAGE, imageBytes)
    }

    private fun sendFrame(type: Byte, payload: ByteArray) {
        synchronized(this) {
            if (!connected) {
                Log.w(TAG, "Skipping send because not connected")
                return
            }

            try {
                val frame = ProtocolFrame.encode(type, payload)
                Log.i(TAG, "Writing frame type=${type.toInt()} length=${payload.size}")
                outputStream?.write(frame)
                outputStream?.flush()
            } catch (e: IOException) {
                Log.e(TAG, "Failed to send frame", e)
                disconnect()
            }
        }
    }

    private fun startReadLoop() {
        val input = inputStream ?: return
        val parser = ProtocolParser(input)
        readThread = thread(start = true, name = "btclip-read-thread") {
            try {
                parser.parseAllFrames(object : ProtocolParser.Listener {
                    override fun onText(text: String) {
                        Log.i(TAG, "Received text from host: $text")
                        listener.onTextReceived(text)
                    }

                    override fun onImage(imageBytes: ByteArray) {
                        Log.i(TAG, "Received image from host (${imageBytes.size} bytes)")
                        listener.onImageReceived(imageBytes)
                    }
                })
            } catch (e: IOException) {
                Log.e(TAG, "Read loop ended", e)
            } finally {
                disconnect()
            }
        }
    }
}
