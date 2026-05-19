package app.shadowboy.netplay.sdk.protocol

public const val MAX_INPUT_BATCH_FRAMES: Int = 4
public const val MAX_INPUT_BATCH_BYTES: Int = 8 * 1024

private val inputBatchMagic = byteArrayOf(0x53, 0x42, 0x49, 0x31)
private const val inputBatchType: Int = 1
private const val batchHeaderBytes: Int = 4 + 1 + 8 + 8 + 1 + 1
private const val frameHeaderBytes: Int = 8 + 2

public data class InputFrameBatch(
    public val roomEpoch: Long,
    public val sessionEpoch: Long,
    public val playerIndex: Int,
    public val frames: List<InputFrame>,
)

public class NetplayInputBatchCodec {
    public fun encode(batch: InputFrameBatch): ByteArray {
        validateBatch(batch)
        val totalBytes = batchHeaderBytes + batch.frames.sumOf { frame ->
            frameHeaderBytes + frame.payload.size
        }
        require(totalBytes <= MAX_INPUT_BATCH_BYTES) { "Netplay input batch is too large." }

        val payload = ByteArray(totalBytes)
        var offset = 0
        inputBatchMagic.copyInto(payload, offset)
        offset += inputBatchMagic.size
        payload[offset] = inputBatchType.toByte()
        offset += 1
        offset = writeLong(payload, offset, batch.roomEpoch)
        offset = writeLong(payload, offset, batch.sessionEpoch)
        payload[offset] = batch.playerIndex.toByte()
        offset += 1
        payload[offset] = batch.frames.size.toByte()
        offset += 1

        for (frame in batch.frames) {
            offset = writeLong(payload, offset, frame.frame)
            offset = writeU16(payload, offset, frame.payload.size)
            for (byte in frame.payload) {
                payload[offset] = byte.toByte()
                offset += 1
            }
        }

        return payload
    }

    public fun decode(payload: ByteArray): InputFrameBatch {
        require(payload.size in batchHeaderBytes..MAX_INPUT_BATCH_BYTES) {
            "Netplay input batch is malformed."
        }
        require(inputBatchMagic.indices.all { index -> payload[index] == inputBatchMagic[index] }) {
            "Netplay input batch type is unsupported."
        }
        require(payload[4].toInt() and 0xff == inputBatchType) {
            "Netplay input batch type is unsupported."
        }

        var offset = 5
        val roomEpoch = readLong(payload, offset)
        offset += 8
        val sessionEpoch = readLong(payload, offset)
        offset += 8
        val playerIndex = payload[offset].toInt() and 0xff
        offset += 1
        val frameCount = payload[offset].toInt() and 0xff
        offset += 1
        require(frameCount > 0) { "Netplay input batch is empty." }
        require(frameCount <= MAX_INPUT_BATCH_FRAMES) {
            "Netplay input batch contains too many frames."
        }

        val frames = mutableListOf<InputFrame>()
        repeat(frameCount) {
            require(payload.size - offset >= frameHeaderBytes) { "Netplay input batch is malformed." }
            val frame = readLong(payload, offset)
            offset += 8
            val payloadLength = readU16(payload, offset)
            offset += 2
            require(payload.size - offset >= payloadLength) { "Netplay input batch is malformed." }

            frames += InputFrame(
                playerIndex = playerIndex,
                frame = frame,
                payload = payload.copyOfRange(offset, offset + payloadLength)
                    .map { byte -> byte.toInt() and 0xff },
            )
            offset += payloadLength
        }
        require(offset == payload.size) { "Netplay input batch is malformed." }

        return InputFrameBatch(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            playerIndex = playerIndex,
            frames = frames,
        )
    }

    private fun validateBatch(batch: InputFrameBatch) {
        require(batch.roomEpoch >= 0) { "Netplay roomEpoch must be non-negative." }
        require(batch.sessionEpoch >= 0) { "Netplay sessionEpoch must be non-negative." }
        require(batch.playerIndex in 0..0xff) { "Netplay playerIndex must fit in one byte." }
        require(batch.frames.isNotEmpty()) { "Netplay input batch is empty." }
        require(batch.frames.size <= MAX_INPUT_BATCH_FRAMES) {
            "Netplay input batch contains too many frames."
        }

        for (frame in batch.frames) {
            require(frame.frame >= 0) { "Netplay frame must be non-negative." }
            require(frame.playerIndex == batch.playerIndex) {
                "Netplay input frame player does not match batch player."
            }
            require(frame.payload.size <= 0xffff) {
                "Netplay input frame payload is too large."
            }
            require(frame.payload.all { value -> value in 0..0xff }) {
                "Netplay payload byte must fit in one byte."
            }
        }
    }

    private fun writeLong(payload: ByteArray, offset: Int, value: Long): Int {
        var nextOffset = offset
        for (shift in 56 downTo 0 step 8) {
            payload[nextOffset] = ((value ushr shift) and 0xffL).toByte()
            nextOffset += 1
        }
        return nextOffset
    }

    private fun readLong(payload: ByteArray, offset: Int): Long {
        var value = 0L
        for (index in 0 until 8) {
            value = (value shl 8) or (payload[offset + index].toLong() and 0xffL)
        }
        require(value >= 0) { "Netplay input batch integer is out of range." }
        return value
    }

    private fun writeU16(payload: ByteArray, offset: Int, value: Int): Int {
        payload[offset] = ((value ushr 8) and 0xff).toByte()
        payload[offset + 1] = (value and 0xff).toByte()
        return offset + 2
    }

    private fun readU16(payload: ByteArray, offset: Int): Int =
        ((payload[offset].toInt() and 0xff) shl 8) or (payload[offset + 1].toInt() and 0xff)
}
