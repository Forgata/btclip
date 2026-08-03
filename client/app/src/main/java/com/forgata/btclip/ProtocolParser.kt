package com.forgata.btclip

import java.io.EOFException
import java.io.IOException
import java.io.InputStream

class ProtocolParser(private val input: InputStream) {
    interface Listener {
        fun onText(text: String)
        fun onImage(imageBytes: ByteArray)
        
    }

    @Throws(IOException::class)
    fun parseNextFrame(listener: Listener): Boolean {
        val header = readExactly(5) ?: return false
        val type = header[0].toInt() and 0xFF
        val length = ((header[1].toInt() and 0xFF) shl 24) or
            ((header[2].toInt() and 0xFF) shl 16) or
            ((header[3].toInt() and 0xFF) shl 8) or
            (header[4].toInt() and 0xFF)

        if (length < 0 || length > MAX_PAYLOAD_SIZE) {
            throw IOException("Invalid payload length: $length")
        }

        val payload = readExactly(length) ?: throw EOFException("Stream ended while reading payload")

        when (type) {
            TYPE_TEXT -> {
                val text = payload.toString(Charsets.UTF_8)
                listener.onText(text)
            }
            TYPE_IMAGE -> listener.onImage(payload)
            else -> throw IOException("Unsupported frame type: 0x${type.toString(16)}")
        }

        return true
    }

    @Throws(IOException::class)
    fun parseAllFrames(listener: Listener) {
        while (parseNextFrame(listener)) {
            // continue until the stream closes
        }
    }

    @Throws(IOException::class)
    private fun readExactly(length: Int): ByteArray? {
        if (length == 0) {
            return ByteArray(0)
        }

        val buffer = ByteArray(length)
        var offset = 0

        while (offset < length) {
            val read = input.read(buffer, offset, length - offset)
            if (read == -1) {
                return if (offset == 0) null else throw EOFException("Unexpected end of stream after $offset bytes")
            }
            offset += read
        }

        return buffer
    }

    companion object {
        const val TYPE_TEXT = 0x01
        const val TYPE_IMAGE = 0x02
        const val MAX_PAYLOAD_SIZE = 20 * 1024 * 1024
    }
}
