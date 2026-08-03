package com.forgata.btclip

object ProtocolFrame {
    const val TYPE_TEXT: Byte = 0x01
    const val TYPE_IMAGE: Byte = 0x02
    const val MAX_PAYLOAD_SIZE = 20 * 1024 * 1024 // 20 MB

    fun encode(type: Byte, payload: ByteArray): ByteArray {
        val length = payload.size
        val frame = ByteArray(5 + length)
        frame[0] = type
        frame[1] = ((length ushr 24) and 0xFF).toByte()
        frame[2] = ((length ushr 16) and 0xFF).toByte()
        frame[3] = ((length ushr 8) and 0xFF).toByte()
        frame[4] = (length and 0xFF).toByte()
        payload.copyInto(frame, 5)
        return frame
    }
}
