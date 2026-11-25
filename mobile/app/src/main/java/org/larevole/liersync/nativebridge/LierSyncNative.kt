// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.nativebridge

import androidx.compose.runtime.Immutable
import org.json.JSONArray
import org.json.JSONObject

@Immutable
data class NodeInfo(
    val deviceId: String,
    val deviceName: String,
    val fingerprint: String,
    val os: String,
    val p2pPort: Int,
    val receiveDir: String,
    val localIps: List<String> = emptyList()
)

@Immutable
data class PeerItem(
    val deviceId: String,
    val deviceName: String,
    val os: String,
    val addr: String
)

@Immutable
data class ChannelItem(
    val medium: String,
    val available: Boolean,
    val endpointHint: String,
    val bandwidthNote: String
)

@Immutable
data class TrustedPeerItem(
    val deviceId: String,
    val deviceName: String,
    val fingerprint: String,
    val addedAt: String
)

@Immutable
data class SyncPreviewDto(
    val toDownload: Int,
    val toUpload: Int,
    val conflicts: Int,
    val error: String? = null
)

sealed interface MobileEvent {
    data class IncomingFile(
        val fromPeer: String,
        val fileName: String,
        val relPath: String?,
        val savedPath: String,
        val bytes: Long
    ) : MobileEvent

    data class ClipboardReceived(
        val fromPeer: String,
        val preview: String,
        val fullText: String
    ) : MobileEvent

    data class ClipboardImageReceived(
        val fromPeer: String,
        val width: Int,
        val height: Int,
        val pngPath: String
    ) : MobileEvent

    data class SyncActivity(val folderId: String, val message: String) : MobileEvent
    data class PeerPaired(val peer: TrustedPeerItem) : MobileEvent
    data class Error(val message: String) : MobileEvent
}

object LierSyncNative {
    init {
        System.loadLibrary("liersync_android")
    }

    external fun initNode(appFilesDir: String, receiveDir: String, deviceName: String): String
    external fun setExtraLanIpsJson(ipsJson: String)
    external fun getPeersJson(): String
    external fun pollEventsJson(): String
    external fun sendBatchJson(targetAddr: String, pathsJson: String): String
    external fun registerSharedFolder(folderId: String, dirPath: String)
    external fun compareFolderJson(targetAddr: String, folderId: String, dirPath: String): String
    external fun syncFolderJson(targetAddr: String, folderId: String, dirPath: String, directionCode: String): String
    external fun pushClipboardJson(targetAddr: String, text: String, force: Boolean): String
    external fun pushClipboardImageBytes(targetAddr: String, width: Int, height: Int, pngBytes: ByteArray, force: Boolean): String
    external fun pullClipboardJson(targetAddr: String): String
    external fun pingPeerJson(targetAddr: String): String
    external fun getChannelsJson(relayEndpoint: String, lang: String): String
    external fun setSecurityConfigJson(configJson: String): String
    external fun pairWithPeerJson(targetAddr: String, pin: String): String
    external fun generatePairingQrPng(addrStr: String, pinStr: String, sizePx: Int): ByteArray
    external fun parsePairingUriJson(rawUri: String): String

    fun parseNodeInfo(raw: String): NodeInfo? = runCatching {
        val obj = JSONObject(raw)
        if (obj.has("error")) return null
        val ipsArr = obj.optJSONArray("local_ips")
        val ips = if (ipsArr != null) {
            List(ipsArr.length()) { ipsArr.optString(it) }.filter { it.isNotBlank() }
        } else {
            emptyList()
        }
        NodeInfo(
            deviceId = obj.optString("device_id"),
            deviceName = obj.optString("device_name"),
            fingerprint = obj.optString("fingerprint"),
            os = obj.optString("os"),
            p2pPort = obj.optInt("p2p_port", 47789),
            receiveDir = obj.optString("receive_dir"),
            localIps = ips
        )
    }.getOrNull()

    fun parsePeers(raw: String): List<PeerItem> = runCatching {
        val arr = JSONArray(raw)
        List(arr.length()) { idx ->
            val o = arr.getJSONObject(idx)
            PeerItem(
                deviceId = o.optString("device_id"),
                deviceName = o.optString("device_name"),
                os = o.optString("os"),
                addr = o.optString("addr")
            )
        }
    }.getOrDefault(emptyList())

    fun parseChannels(raw: String): List<ChannelItem> = runCatching {
        val arr = JSONArray(raw)
        List(arr.length()) { idx ->
            val o = arr.getJSONObject(idx)
            ChannelItem(
                medium = o.optString("medium"),
                available = o.optBoolean("available"),
                endpointHint = o.optString("endpoint_hint"),
                bandwidthNote = o.optString("bandwidth_note")
            )
        }
    }.getOrDefault(emptyList())

    fun parseEvents(raw: String): List<MobileEvent> = runCatching {
        val arr = JSONArray(raw)
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.getJSONObject(i)
                when (o.optString("kind")) {
                    "IncomingFile" -> add(
                        MobileEvent.IncomingFile(
                            fromPeer = o.optString("from_peer"),
                            fileName = o.optString("file_name"),
                            relPath = o.optString("rel_path").takeIf { it.isNotBlank() && it != "null" },
                            savedPath = o.optString("saved_path"),
                            bytes = o.optLong("bytes")
                        )
                    )
                    "ClipboardReceived" -> add(
                        MobileEvent.ClipboardReceived(
                            fromPeer = o.optString("from_peer"),
                            preview = o.optString("text_preview"),
                            fullText = o.optString("full_text")
                        )
                    )
                    "ClipboardImageReceived" -> add(
                        MobileEvent.ClipboardImageReceived(
                            fromPeer = o.optString("from_peer"),
                            width = o.optInt("width"),
                            height = o.optInt("height"),
                            pngPath = o.optString("png_path")
                        )
                    )
                    "SyncActivity" -> add(
                        MobileEvent.SyncActivity(
                            folderId = o.optString("folder_id"),
                            message = o.optString("message")
                        )
                    )
                    "PeerPaired" -> add(
                        MobileEvent.PeerPaired(
                            TrustedPeerItem(
                                deviceId = o.optString("device_id"),
                                deviceName = o.optString("device_name"),
                                fingerprint = o.optString("fingerprint"),
                                addedAt = o.optString("added_at")
                            )
                        )
                    )
                    "Error" -> add(MobileEvent.Error(o.optString("message")))
                }
            }
        }
    }.getOrDefault(emptyList())

    fun parsePreview(raw: String): SyncPreviewDto = runCatching {
        val o = JSONObject(raw)
        if (o.has("error")) {
            SyncPreviewDto(0, 0, 0, o.optString("error"))
        } else {
            SyncPreviewDto(
                toDownload = o.optJSONArray("to_download")?.length() ?: 0,
                toUpload = o.optJSONArray("to_upload")?.length() ?: 0,
                conflicts = o.optJSONArray("conflicts")?.length() ?: 0
            )
        }
    }.getOrElse { SyncPreviewDto(0, 0, 0, it.message) }
}
