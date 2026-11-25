// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.service

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothServerSocket
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.compose.runtime.Immutable
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import org.larevole.liersync.nativebridge.NodeInfo
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.io.FileOutputStream
import java.security.MessageDigest
import java.util.UUID
import kotlin.concurrent.thread

@Immutable
data class BtDeviceItem(
    val name: String,
    val mac: String
)

object BluetoothSyncManager {
    val LIERSYNC_UUID: UUID = UUID.fromString("7a3f21b0-4c8e-4e9b-9e12-a5b6c7d8e901")
    private const val MAX_FRAME = 16 * 1024 * 1024

    @Volatile
    private var serverRunning = false
    @Volatile
    private var serverSocket: BluetoothServerSocket? = null

    fun hasConnectPermission(context: Context): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
                PackageManager.PERMISSION_GRANTED
        } else {
            true
        }
    }

    @SuppressLint("MissingPermission")
    fun listPairedDevices(context: Context): List<BtDeviceItem> {
        if (!hasConnectPermission(context)) return emptyList()
        return runCatching {
            val mgr = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
            val adapter = mgr?.adapter ?: return@runCatching emptyList()
            adapter.bondedDevices
                ?.map { dev -> BtDeviceItem(name = dev.name ?: dev.address, mac = dev.address) }
                ?.sortedBy { it.name.lowercase() }
                ?: emptyList()
        }.getOrDefault(emptyList())
    }

    @SuppressLint("MissingPermission")
    fun startServer(
        context: Context,
        stagingDir: File,
        onClipboardText: (peer: String, text: String) -> Unit,
        onClipboardImage: (peer: String, width: Int, height: Int, pngPath: String) -> Unit,
        onIncomingFile: (peer: String, fileName: String, relPath: String?, savedPath: String, bytes: Long) -> Unit
    ) {
        if (serverRunning || !hasConnectPermission(context)) return
        val mgr = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager ?: return
        val adapter: BluetoothAdapter = mgr.adapter ?: return
        if (!adapter.isEnabled) return

        serverRunning = true
        thread(name = "liersync-bt-srv", isDaemon = true) {
            while (serverRunning) {
                runCatching {
                    val srv = adapter.listenUsingRfcommWithServiceRecord("LierSync", LIERSYNC_UUID)
                    serverSocket = srv
                    while (serverRunning) {
                        val client = srv.accept() ?: continue
                        thread(isDaemon = true) {
                            handleClientSocket(
                                client = client,
                                stagingDir = stagingDir,
                                onClipboardText = onClipboardText,
                                onClipboardImage = onClipboardImage,
                                onIncomingFile = onIncomingFile
                            )
                        }
                    }
                }
                if (serverRunning) {
                    Thread.sleep(3000)
                }
            }
        }
    }

    fun stopServer() {
        serverRunning = false
        runCatching { serverSocket?.close() }
        serverSocket = null
    }

    private fun writeFrame(out: DataOutputStream, obj: JSONObject) {
        val bytes = obj.toString().toByteArray(Charsets.UTF_8)
        out.writeInt(bytes.size)
        out.write(bytes)
        out.flush()
    }

    private fun readFrame(inp: DataInputStream): JSONObject? {
        val len = runCatching { inp.readInt() }.getOrNull() ?: return null
        if (len <= 0 || len > MAX_FRAME) return null
        val buf = ByteArray(len)
        inp.readFully(buf)
        return runCatching { JSONObject(String(buf, Charsets.UTF_8)) }.getOrNull()
    }

    private fun handleClientSocket(
        client: BluetoothSocket,
        stagingDir: File,
        onClipboardText: (String, String) -> Unit,
        onClipboardImage: (String, Int, Int, String) -> Unit,
        onIncomingFile: (String, String, String?, String, Long) -> Unit
    ) {
        client.use { sock ->
            val inp = DataInputStream(sock.inputStream)
            val out = DataOutputStream(sock.outputStream)
            var peerName = sock.remoteDevice?.address ?: "Bluetooth"
            val transfers = mutableMapOf<String, Triple<File, FileOutputStream, Long>>()
            val transferNames = mutableMapOf<String, Pair<String, String?>>()

            while (true) {
                val frame = readFrame(inp) ?: break
                when {
                    frame.has("Hello") -> {
                        val h = frame.getJSONObject("Hello")
                        peerName = h.optString("device_name", peerName)
                        val node = LierSyncState.nodeInfo.value
                        writeFrame(
                            out,
                            JSONObject().put(
                                "Hello",
                                JSONObject().apply {
                                    put("device_id", node?.deviceId ?: "android")
                                    put("device_name", node?.deviceName ?: "Android")
                                    put("os", "android")
                                    put("fingerprint", node?.fingerprint ?: "")
                                    put("p2p_port", node?.p2pPort ?: 47789)
                                }
                            )
                        )
                    }
                    frame.has("ClipboardPush") -> {
                        val cp = frame.getJSONObject("ClipboardPush")
                        val sender = cp.optString("sender_name", peerName)
                        val text = cp.optString("text")
                        if (text.isNotEmpty()) {
                            onClipboardText("$sender (BT)", text)
                        }
                        writeFrame(out, JSONObject().put("Ack", JSONObject().put("message", "OK")))
                    }
                    frame.has("ClipboardImagePush") -> {
                        val ci = frame.getJSONObject("ClipboardImagePush")
                        val sender = ci.optString("sender_name", peerName)
                        val w = ci.optInt("width", 0)
                        val h = ci.optInt("height", 0)
                        val bytesArr = ci.optJSONArray("png_bytes")
                        if (bytesArr != null) {
                            val png = ByteArray(bytesArr.length()) { i -> bytesArr.getInt(i).toByte() }
                            stagingDir.mkdirs()
                            val outFile = File(stagingDir, "clip_bt_${System.currentTimeMillis()}.png")
                            outFile.writeBytes(png)
                            onClipboardImage("$sender (BT)", w, h, outFile.absolutePath)
                        }
                        writeFrame(out, JSONObject().put("Ack", JSONObject().put("message", "OK")))
                    }
                    frame.has("QuickSendOffer") -> {
                        val q = frame.getJSONObject("QuickSendOffer")
                        val tid = q.optString("transfer_id")
                        val fname = File(q.optString("file_name", "bt_file")).name
                        val rel = q.optString("rel_path").takeIf { it.isNotBlank() && it != "null" }
                        val sz = q.optLong("file_size", 0L)
                        stagingDir.mkdirs()
                        val dest = File(stagingDir, "${System.nanoTime()}_$fname")
                        val fos = FileOutputStream(dest)
                        transfers[tid] = Triple(dest, fos, sz)
                        transferNames[tid] = fname to rel
                        writeFrame(
                            out,
                            JSONObject().put(
                                "QuickSendAccept",
                                JSONObject().put("transfer_id", tid).put("accepted", true)
                            )
                        )
                    }
                    frame.has("FileChunk") -> {
                        val c = frame.getJSONObject("FileChunk")
                        val tid = c.optString("transfer_id")
                        val dataArr = c.optJSONArray("data")
                        val entry = transfers[tid]
                        if (entry != null && dataArr != null) {
                            val chunk = ByteArray(dataArr.length()) { i -> dataArr.getInt(i).toByte() }
                            entry.second.write(chunk)
                        }
                    }
                    frame.has("QuickSendDone") -> {
                        val d = frame.getJSONObject("QuickSendDone")
                        val tid = d.optString("transfer_id")
                        val entry = transfers.remove(tid)
                        val meta = transferNames.remove(tid)
                        if (entry != null) {
                            entry.second.flush()
                            entry.second.close()
                            val fname = meta?.first ?: entry.first.name
                            onIncomingFile("$peerName (BT)", fname, meta?.second, entry.first.absolutePath, entry.third)
                        }
                        writeFrame(out, JSONObject().put("Ack", JSONObject().put("message", "OK")))
                    }
                }
            }
            transfers.values.forEach { runCatching { it.second.close() } }
        }
    }

    @SuppressLint("MissingPermission")
    private fun <T> withClientSocket(context: Context, mac: String, block: (DataInputStream, DataOutputStream, NodeInfo?) -> T): Result<T> = runCatching {
        if (!hasConnectPermission(context)) error("Missing BLUETOOTH_CONNECT permission")
        val mgr = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
            ?: error("Bluetooth service unavailable")
        val dev = mgr.adapter?.getRemoteDevice(mac) ?: error("Bluetooth device not found: $mac")
        val sock = dev.createRfcommSocketToServiceRecord(LIERSYNC_UUID)
        sock.connect()
        sock.use { s ->
            val out = DataOutputStream(s.outputStream)
            val inp = DataInputStream(s.inputStream)
            val node = LierSyncState.nodeInfo.value
            writeFrame(
                out,
                JSONObject().put(
                    "Hello",
                    JSONObject().apply {
                        put("device_id", node?.deviceId ?: "android")
                        put("device_name", node?.deviceName ?: "Android")
                        put("os", "android")
                        put("fingerprint", node?.fingerprint ?: "")
                        put("p2p_port", node?.p2pPort ?: 47789)
                    }
                )
            )
            readFrame(inp)
            block(inp, out, node)
        }
    }

    private fun sha256Hex(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    fun sendClipboardText(context: Context, mac: String, text: String): Result<Unit> =
        withClientSocket(context, mac) { inp, out, node ->
            val sha = sha256Hex(text.toByteArray(Charsets.UTF_8))
            writeFrame(
                out,
                JSONObject().put(
                    "ClipboardPush",
                    JSONObject().apply {
                        put("sender_id", node?.deviceId ?: "android")
                        put("sender_name", node?.deviceName ?: "Android")
                        put("sha256", sha)
                        put("text", text)
                    }
                )
            )
            val reply = readFrame(inp)
            if (reply?.has("Error") == true) {
                error(reply.getJSONObject("Error").optString("message", "Bluetooth error"))
            }
        }

    fun sendClipboardImage(context: Context, mac: String, width: Int, height: Int, pngBytes: ByteArray): Result<Unit> =
        withClientSocket(context, mac) { inp, out, node ->
            val sha = sha256Hex(pngBytes)
            val byteArr = JSONArray()
            for (b in pngBytes) byteArr.put(b.toInt() and 0xFF)
            writeFrame(
                out,
                JSONObject().put(
                    "ClipboardImagePush",
                    JSONObject().apply {
                        put("sender_id", node?.deviceId ?: "android")
                        put("sender_name", node?.deviceName ?: "Android")
                        put("sha256", sha)
                        put("width", width)
                        put("height", height)
                        put("png_bytes", byteArr)
                    }
                )
            )
            val reply = readFrame(inp)
            if (reply?.has("Error") == true) {
                error(reply.getJSONObject("Error").optString("message", "Bluetooth error"))
            }
        }

    fun sendFiles(context: Context, mac: String, paths: List<String>): Result<Int> =
        withClientSocket(context, mac) { inp, out, _ ->
            var sentCount = 0
            val chunkBuf = ByteArray(64 * 1024)
            for (p in paths) {
                val f = File(p)
                if (!f.exists() || !f.isFile) continue
                val bytes = f.readBytes()
                val sha = sha256Hex(bytes)
                val tid = UUID.randomUUID().toString()
                writeFrame(
                    out,
                    JSONObject().put(
                        "QuickSendOffer",
                        JSONObject().apply {
                            put("transfer_id", tid)
                            put("file_name", f.name)
                            put("rel_path", f.name)
                            put("file_size", f.length())
                            put("sha256", sha)
                        }
                    )
                )
                val acc = readFrame(inp)
                if (acc?.optJSONObject("QuickSendAccept")?.optBoolean("accepted") != true) {
                    error("Peer rejected Bluetooth transfer")
                }
                f.inputStream().use { fis ->
                    var offset = 0L
                    while (true) {
                        val n = fis.read(chunkBuf)
                        if (n <= 0) break
                        val jArr = JSONArray()
                        for (i in 0 until n) jArr.put(chunkBuf[i].toInt() and 0xFF)
                        writeFrame(
                            out,
                            JSONObject().put(
                                "FileChunk",
                                JSONObject().apply {
                                    put("transfer_id", tid)
                                    put("offset", offset)
                                    put("data", jArr)
                                }
                            )
                        )
                        offset += n
                    }
                }
                writeFrame(
                    out,
                    JSONObject().put("QuickSendDone", JSONObject().put("transfer_id", tid))
                )
                readFrame(inp)
                sentCount++
            }
            sentCount
        }
}
