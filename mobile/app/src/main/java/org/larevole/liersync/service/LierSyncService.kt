// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Environment
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.webkit.MimeTypeMap
import android.widget.Toast
import androidx.compose.runtime.Immutable
import androidx.core.content.FileProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import org.larevole.liersync.nativebridge.ChannelItem
import org.larevole.liersync.nativebridge.LierSyncNative
import org.larevole.liersync.nativebridge.MobileEvent
import org.larevole.liersync.nativebridge.NodeInfo
import org.larevole.liersync.nativebridge.PeerItem
import org.larevole.liersync.nativebridge.SyncPreviewDto
import org.larevole.liersync.nativebridge.TrustedPeerItem
import org.larevole.liersync.ui.ClipboardSyncActivity
import org.larevole.liersync.ui.MainActivity
import java.io.ByteArrayOutputStream
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Immutable
data class ClipboardRecord(
    val time: String,
    val direction: String,
    val peer: String,
    val preview: String
)

@Immutable
data class TransferHistoryItem(
    val id: String,
    val time: String,
    val fileName: String,
    val peer: String,
    val displayPath: String,
    val contentUri: String?,
    val bytes: Long,
    val isSent: Boolean
)

fun tr(lang: String, ru: String, en: String): String = if (lang == "en") en else ru

@Immutable
data class MobileSharedFolder(
    val id: String,
    val dirPath: String,
    val remoteAddr: String,
    val autoSync: Boolean,
    val directionCode: String, // "bidirectional" | "send_only" | "receive_only"
    val status: String,
    val preview: SyncPreviewDto? = null,
    val safTreeUri: String? = null
) {
    fun directionLabel(lang: String = LierSyncState.language.value): String = when (directionCode) {
        "send_only" -> tr(lang, "Отправитель", "Send only")
        "receive_only" -> tr(lang, "Получатель", "Receive only")
        else -> tr(lang, "Общая", "Both")
    }
}

object LierSyncState {
    private val _nodeInfo = MutableStateFlow<NodeInfo?>(null)
    val nodeInfo = _nodeInfo.asStateFlow()

    private val _peers = MutableStateFlow<List<PeerItem>>(emptyList())
    val peers = _peers.asStateFlow()

    private val _channels = MutableStateFlow<List<ChannelItem>>(emptyList())
    val channels = _channels.asStateFlow()

    private val _logs = MutableStateFlow<List<String>>(emptyList())
    val logs = _logs.asStateFlow()

    private val _transferHistory = MutableStateFlow<List<TransferHistoryItem>>(emptyList())
    val transferHistory = _transferHistory.asStateFlow()

    private val _clipboardHistory = MutableStateFlow<List<ClipboardRecord>>(emptyList())
    val clipboardHistory = _clipboardHistory.asStateFlow()

    private val _autoClipboard = MutableStateFlow(false)
    val autoClipboard = _autoClipboard.asStateFlow()

    private val _sharedFolders = MutableStateFlow<List<MobileSharedFolder>>(emptyList())
    val sharedFolders = _sharedFolders.asStateFlow()

    private val _targetAddr = MutableStateFlow("192.168.43.100:47789")
    val targetAddr = _targetAddr.asStateFlow()

    private val _channelPolicy = MutableStateFlow("Авто (Wi-Fi -> USB/Direct -> Bluetooth -> Relay)")
    val channelPolicy = _channelPolicy.asStateFlow()

    private val _relayEndpoint = MutableStateFlow("")
    val relayEndpoint = _relayEndpoint.asStateFlow()

    private val _language = MutableStateFlow("ru")
    val language = _language.asStateFlow()

    private val _encryptTransport = MutableStateFlow(true)
    val encryptTransport = _encryptTransport.asStateFlow()

    private val _whitelistOnly = MutableStateFlow(false)
    val whitelistOnly = _whitelistOnly.asStateFlow()

    private val _pairingPin = MutableStateFlow(
        String.format(Locale.US, "%06d", java.security.SecureRandom().nextInt(1_000_000))
    )
    val pairingPin = _pairingPin.asStateFlow()

    private val _trustedPeers = MutableStateFlow<List<TrustedPeerItem>>(emptyList())
    val trustedPeers = _trustedPeers.asStateFlow()

    private val _btDevices = MutableStateFlow<List<BtDeviceItem>>(emptyList())
    val btDevices = _btDevices.asStateFlow()

    private val _selectedBtMac = MutableStateFlow("")
    val selectedBtMac = _selectedBtMac.asStateFlow()

    fun setBtDevices(list: List<BtDeviceItem>) {
        _btDevices.value = list
        if (_selectedBtMac.value.isBlank() && list.isNotEmpty()) {
            _selectedBtMac.value = list.first().mac
        }
    }

    fun selectBtMac(mac: String) {
        _selectedBtMac.value = mac
    }

    private val _pendingShareFiles = MutableStateFlow<List<String>>(emptyList())
    val pendingShareFiles = _pendingShareFiles.asStateFlow()

    fun addPendingShareFiles(paths: List<String>) {
        _pendingShareFiles.value = (_pendingShareFiles.value + paths).distinct()
    }

    fun consumePendingShareFiles(): List<String> {
        val list = _pendingShareFiles.value
        _pendingShareFiles.value = emptyList()
        return list
    }

    @Volatile
    var isUiVisible: Boolean = true

    @Volatile
    private var configFile: File? = null

    @Volatile
    internal var onLanguageUpdated: (() -> Unit)? = null

    fun initPersistence(filesDir: File) {
        configFile = File(filesDir, "mobile_config.json")
        loadFromDisk()
    }

    private fun loadFromDisk() {
        val file = configFile ?: return
        if (!file.exists()) return
        runCatching {
            val root = JSONObject(file.readText())
            _language.value = if (root.optString("language") == "en") "en" else "ru"
            _autoClipboard.value = root.optBoolean("auto_clipboard", false)
            root.optString("target_addr").takeIf { it.isNotBlank() }?.let { _targetAddr.value = it }
            root.optString("channel_policy").takeIf { it.isNotBlank() }?.let { _channelPolicy.value = it }
            _relayEndpoint.value = root.optString("relay_endpoint", "")
            _encryptTransport.value = root.optBoolean("encrypt_transport", true)
            _whitelistOnly.value = root.optBoolean("whitelist_only", false)
            root.optString("pairing_pin").takeIf { it.isNotBlank() }?.let { _pairingPin.value = it }

            val tpArr = root.optJSONArray("trusted_peers")
            if (tpArr != null) {
                val tpList = buildList {
                    for (i in 0 until tpArr.length()) {
                        val o = tpArr.getJSONObject(i)
                        val did = o.optString("device_id")
                        val fp = o.optString("fingerprint")
                        if (did.isNotBlank() || fp.isNotBlank()) {
                            add(
                                TrustedPeerItem(
                                    deviceId = did,
                                    deviceName = o.optString("device_name"),
                                    fingerprint = fp,
                                    addedAt = o.optString("added_at")
                                )
                            )
                        }
                    }
                }
                _trustedPeers.value = tpList
            }

            val foldersArr = root.optJSONArray("shared_folders")
            if (foldersArr != null) {
                val lang = _language.value
                val list = buildList {
                    for (i in 0 until foldersArr.length()) {
                        val o = foldersArr.getJSONObject(i)
                        val fid = o.optString("id")
                        val dirPath = o.optString("dir_path")
                        if (fid.isNotBlank() && dirPath.isNotBlank()) {
                            File(dirPath).mkdirs()
                            LierSyncNative.registerSharedFolder(fid, dirPath)
                            val dirCode = o.optString("direction_code", "bidirectional")
                            val auto = o.optBoolean("auto_sync", true)
                            val safUri = o.optString("saf_tree_uri").takeIf { it.isNotBlank() }
                            val f = MobileSharedFolder(
                                id = fid,
                                dirPath = dirPath,
                                remoteAddr = o.optString("remote_addr", _targetAddr.value),
                                autoSync = auto,
                                directionCode = dirCode,
                                status = if (auto) tr(lang, "Авто", "Auto") else tr(lang, "Ручной", "Manual"),
                                safTreeUri = safUri
                            )
                            add(f.copy(status = "${f.status} · ${f.directionLabel(lang)}"))
                        }
                    }
                }
                _sharedFolders.value = list
            }

            val histArr = root.optJSONArray("transfer_history")
            if (histArr != null) {
                val list = buildList {
                    for (i in 0 until histArr.length()) {
                        val o = histArr.getJSONObject(i)
                        add(
                            TransferHistoryItem(
                                id = o.optString("id", "h_$i"),
                                time = o.optString("time"),
                                fileName = o.optString("file_name"),
                                peer = o.optString("peer"),
                                displayPath = o.optString("display_path"),
                                contentUri = o.optString("content_uri").takeIf { it.isNotBlank() && it != "null" },
                                bytes = o.optLong("bytes"),
                                isSent = o.optBoolean("is_sent", false)
                            )
                        )
                    }
                }
                _transferHistory.value = list
            }
        }
    }

    @Synchronized
    fun saveToDisk() {
        val file = configFile ?: return
        runCatching {
            val root = JSONObject().apply {
                put("language", _language.value)
                put("auto_clipboard", _autoClipboard.value)
                put("target_addr", _targetAddr.value)
                put("channel_policy", _channelPolicy.value)
                put("relay_endpoint", _relayEndpoint.value)
                put("encrypt_transport", _encryptTransport.value)
                put("whitelist_only", _whitelistOnly.value)
                put("pairing_pin", _pairingPin.value)

                val tpArr = JSONArray()
                for (tp in _trustedPeers.value) {
                    tpArr.put(
                        JSONObject().apply {
                            put("device_id", tp.deviceId)
                            put("device_name", tp.deviceName)
                            put("fingerprint", tp.fingerprint)
                            put("added_at", tp.addedAt)
                        }
                    )
                }
                put("trusted_peers", tpArr)

                val fArr = JSONArray()
                for (f in _sharedFolders.value) {
                    fArr.put(
                        JSONObject().apply {
                            put("id", f.id)
                            put("dir_path", f.dirPath)
                            put("remote_addr", f.remoteAddr)
                            put("auto_sync", f.autoSync)
                            put("direction_code", f.directionCode)
                            if (!f.safTreeUri.isNullOrBlank()) {
                                put("saf_tree_uri", f.safTreeUri)
                            }
                        }
                    )
                }
                put("shared_folders", fArr)

                val hArr = JSONArray()
                for (h in _transferHistory.value.take(60)) {
                    hArr.put(
                        JSONObject().apply {
                            put("id", h.id)
                            put("time", h.time)
                            put("file_name", h.fileName)
                            put("peer", h.peer)
                            put("display_path", h.displayPath)
                            put("content_uri", h.contentUri ?: "")
                            put("bytes", h.bytes)
                            put("is_sent", h.isSent)
                        }
                    )
                }
                put("transfer_history", hArr)
            }
            file.writeText(root.toString())
        }
    }

    fun setNodeInfo(info: NodeInfo?) {
        if (_nodeInfo.value != info) _nodeInfo.value = info
    }

    fun setPeers(list: List<PeerItem>) {
        if (_peers.value != list) _peers.value = list
    }

    fun setChannels(list: List<ChannelItem>) {
        if (_channels.value != list) _channels.value = list
    }

    fun setAutoClipboard(enabled: Boolean) {
        _autoClipboard.value = enabled
        saveToDisk()
        val l = _language.value
        addLog(
            if (enabled) tr(l, "Автосинхронизация буфера включена", "Auto-clipboard enabled")
            else tr(l, "Автосинхронизация буфера выключена", "Auto-clipboard disabled")
        )
    }

    fun setLanguage(lang: String) {
        val norm = if (lang == "en") "en" else "ru"
        if (_language.value == norm) return
        _language.value = norm
        _sharedFolders.value = _sharedFolders.value.map { f ->
            val mode = if (f.autoSync) tr(norm, "Авто", "Auto") else tr(norm, "Ручной", "Manual")
            f.copy(status = "$mode · ${f.directionLabel(norm)}")
        }
        setChannels(
            LierSyncNative.parseChannels(
                LierSyncNative.getChannelsJson(_relayEndpoint.value, norm)
            )
        )
        saveToDisk()
        addLog(tr(norm, "Язык: Русский", "Language: English"))
        onLanguageUpdated?.invoke()
    }

    fun setTargetAddr(addr: String) {
        if (_targetAddr.value != addr) {
            _targetAddr.value = addr
            saveToDisk()
        }
    }

    fun setChannelPolicy(policy: String) {
        _channelPolicy.value = policy
        saveToDisk()
        addLog("${tr(_language.value, "Режим канала", "Channel mode")}: $policy")
    }

    fun setRelayEndpoint(endpoint: String) {
        _relayEndpoint.value = endpoint
        setChannels(
            LierSyncNative.parseChannels(
                LierSyncNative.getChannelsJson(endpoint, _language.value)
            )
        )
        saveToDisk()
    }

    fun addLog(msg: String) {
        val ts = SimpleDateFormat("HH:mm:ss", Locale.getDefault()).format(Date())
        _logs.value = (listOf("[$ts] $msg") + _logs.value).take(150)
    }

    fun addTransferRecord(
        fileName: String,
        peer: String,
        displayPath: String,
        contentUri: String?,
        bytes: Long,
        isSent: Boolean
    ) {
        val ts = SimpleDateFormat("HH:mm:ss", Locale.getDefault()).format(Date())
        val item = TransferHistoryItem(
            id = "${System.nanoTime()}",
            time = ts,
            fileName = fileName,
            peer = peer,
            displayPath = displayPath,
            contentUri = contentUri,
            bytes = bytes,
            isSent = isSent
        )
        _transferHistory.value = (listOf(item) + _transferHistory.value).take(60)
        saveToDisk()
    }

    fun clearTransferHistory() {
        _transferHistory.value = emptyList()
        saveToDisk()
    }

    fun addClipboardRecord(direction: String, peer: String, preview: String) {
        val ts = SimpleDateFormat("HH:mm:ss", Locale.getDefault()).format(Date())
        _clipboardHistory.value = (listOf(ClipboardRecord(ts, direction, peer, preview)) + _clipboardHistory.value).take(50)
    }

    fun addOrUpdateFolder(folder: MobileSharedFolder) {
        val current = _sharedFolders.value.toMutableList()
        val idx = current.indexOfFirst { it.id == folder.id }
        if (idx >= 0) current[idx] = folder else current.add(folder)
        _sharedFolders.value = current
        saveToDisk()
    }

    fun removeFolder(folderId: String) {
        _sharedFolders.value = _sharedFolders.value.filterNot { it.id == folderId }
        saveToDisk()
    }

    fun setEncryptTransport(enabled: Boolean) {
        _encryptTransport.value = enabled
        saveToDisk()
        pushSecurityToNative()
    }

    fun setWhitelistOnly(enabled: Boolean) {
        _whitelistOnly.value = enabled
        saveToDisk()
        pushSecurityToNative()
    }

    fun rotatePairingPin() {
        _pairingPin.value = String.format(Locale.US, "%06d", java.security.SecureRandom().nextInt(1_000_000))
        saveToDisk()
        pushSecurityToNative()
    }

    fun addTrustedPeer(peer: TrustedPeerItem) {
        val current = _trustedPeers.value.toMutableList()
        val idx = current.indexOfFirst {
            (peer.deviceId.isNotBlank() && it.deviceId == peer.deviceId) ||
                (peer.fingerprint.isNotBlank() && it.fingerprint == peer.fingerprint)
        }
        if (idx >= 0) current[idx] = peer else current.add(peer)
        _trustedPeers.value = current
        saveToDisk()
        pushSecurityToNative()
    }

    fun removeTrustedPeer(idOrFp: String) {
        _trustedPeers.value = _trustedPeers.value.filterNot {
            it.deviceId == idOrFp || it.fingerprint == idOrFp
        }
        saveToDisk()
        pushSecurityToNative()
    }

    fun pushSecurityToNative() {
        val obj = JSONObject().apply {
            put("encrypt_transport", _encryptTransport.value)
            put("whitelist_only", _whitelistOnly.value)
            put("pairing_pin", _pairingPin.value)
            val arr = JSONArray()
            for (tp in _trustedPeers.value) {
                arr.put(
                    JSONObject().apply {
                        put("device_id", tp.deviceId)
                        put("device_name", tp.deviceName)
                        put("fingerprint", tp.fingerprint)
                        put("added_at", tp.addedAt)
                    }
                )
            }
            put("trusted_peers", arr)
        }
        LierSyncNative.setSecurityConfigJson(obj.toString())
    }
}

class LierSyncService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var multicastLock: WifiManager.MulticastLock? = null
    private var clipboardManager: ClipboardManager? = null
    private val mainHandler = Handler(Looper.getMainLooper())
    @Volatile
    private var suppressNextClipChange = false
    private val folderSnapshots = mutableMapOf<String, Map<String, Pair<Long, Long>>>()

    private val clipListener = ClipboardManager.OnPrimaryClipChangedListener {
        if (suppressNextClipChange) {
            suppressNextClipChange = false
            return@OnPrimaryClipChangedListener
        }
        if (!LierSyncState.autoClipboard.value) return@OnPrimaryClipChangedListener
        readPrimaryClipboardImagePng()?.let { (w, h, png) ->
            pushImageToActivePeers(w, h, png)
            return@OnPrimaryClipChangedListener
        }
        val text = readPrimaryClipboardText() ?: return@OnPrimaryClipChangedListener
        pushTextToActivePeers(text)
    }

    override fun onCreate() {
        super.onCreate()
        val stagingInbox = File(cacheDir, "inbox_tmp").apply { mkdirs() }
        val deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
        val rawInfo = LierSyncNative.initNode(filesDir.absolutePath, stagingInbox.absolutePath, deviceName)
        LierSyncState.initPersistence(filesDir)
        LierSyncState.pushSecurityToNative()
        LierSyncState.onLanguageUpdated = {
            mainHandler.post { startForegroundCompat() }
        }
        startForegroundCompat()
        updateMulticastLock(true)

        val lanIps = detectLanIpv4Addresses()
        if (lanIps.isNotEmpty()) {
            LierSyncNative.setExtraLanIpsJson(JSONArray(lanIps).toString())
        }
        val parsed = LierSyncNative.parseNodeInfo(rawInfo)
        val info = parsed?.copy(localIps = (lanIps + parsed.localIps).distinct())
        LierSyncState.setNodeInfo(info)
        LierSyncState.setChannels(
            LierSyncNative.parseChannels(
                LierSyncNative.getChannelsJson(
                    LierSyncState.relayEndpoint.value,
                    LierSyncState.language.value
                )
            )
        )
        val lang = LierSyncState.language.value
        if (info != null) {
            LierSyncState.addLog(
                "${tr(lang, "Узел запущен", "Node started")}: ${info.deviceName} (${tr(lang, "порт", "port")} ${info.p2pPort})"
            )
        } else {
            LierSyncState.addLog("${tr(lang, "Ошибка запуска ядра", "Core startup error")}: $rawInfo")
        }

        clipboardManager = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        clipboardManager?.addPrimaryClipChangedListener(clipListener)

        LierSyncState.setBtDevices(BluetoothSyncManager.listPairedDevices(this))
        BluetoothSyncManager.startServer(
            context = this,
            stagingDir = stagingInbox,
            onClipboardText = { peer, text ->
                suppressNextClipChange = true
                mainHandler.post { writePrimaryClipboardText(text) }
                LierSyncState.addClipboardRecord("Получено", peer, text.take(80))
                LierSyncState.addLog("${tr(lang, "Буфер от", "Clipboard from")} $peer")
            },
            onClipboardImage = { peer, w, h, pngPath ->
                suppressNextClipChange = true
                mainHandler.post { writePrimaryClipboardImageFile(pngPath) }
                LierSyncState.addClipboardRecord("Получено", peer, "[${tr(lang, "Картинка", "Image")} ${w}x${h}]")
                LierSyncState.addLog("${tr(lang, "Картинка от", "Image from")} $peer (${w}x${h})")
            },
            onIncomingFile = { peer, fileName, relPath, savedPath, bytes ->
                val (savedDisplay, uriStr) = exportToPublicDownloads(savedPath, fileName, relPath)
                LierSyncState.addTransferRecord(fileName, peer, savedDisplay, uriStr, bytes, false)
                LierSyncState.addLog("${tr(lang, "Получен", "Received")} '$fileName' ($peer) -> $savedDisplay")
            }
        )

        scope.launch {
            var tick = 0
            while (isActive) {
                val activeUi = LierSyncState.isUiVisible
                updateMulticastLock(activeUi)

                if (activeUi || tick % 3 == 0) {
                    val peers = LierSyncNative.parsePeers(LierSyncNative.getPeersJson())
                    LierSyncState.setPeers(peers)
                }

                val events = LierSyncNative.parseEvents(LierSyncNative.pollEventsJson())
                for (ev in events) {
                    when (ev) {
                        is MobileEvent.IncomingFile -> {
                            val l = LierSyncState.language.value
                            val (savedDisplay, uriStr) = exportToPublicDownloads(ev.savedPath, ev.fileName, ev.relPath)
                            LierSyncState.addTransferRecord(ev.fileName, ev.fromPeer, savedDisplay, uriStr, ev.bytes, false)
                            LierSyncState.addLog(
                                "${tr(l, "Получен", "Received")} '${ev.fileName}' (${ev.bytes} ${tr(l, "Б", "B")}) -> $savedDisplay"
                            )
                        }
                        is MobileEvent.ClipboardReceived -> {
                            val l = LierSyncState.language.value
                            suppressNextClipChange = true
                            writePrimaryClipboardText(ev.fullText)
                            LierSyncState.addClipboardRecord("Получено", ev.fromPeer, ev.preview)
                            LierSyncState.addLog("${tr(l, "Буфер от", "Clipboard from")} ${ev.fromPeer}: ${ev.preview}")
                        }
                        is MobileEvent.ClipboardImageReceived -> {
                            val l = LierSyncState.language.value
                            suppressNextClipChange = true
                            writePrimaryClipboardImageFile(ev.pngPath)
                            val label = "[${tr(l, "Картинка", "Image")} ${ev.width}x${ev.height}]"
                            LierSyncState.addClipboardRecord("Получено", ev.fromPeer, label)
                            LierSyncState.addLog("${tr(l, "Картинка буфера от", "Clipboard image from")} ${ev.fromPeer}: ${ev.width}x${ev.height}")
                        }
                        is MobileEvent.SyncActivity -> {
                            LierSyncState.addLog("[${ev.folderId}] ${ev.message}")
                        }
                        is MobileEvent.PeerPaired -> {
                            val l = LierSyncState.language.value
                            LierSyncState.addTrustedPeer(ev.peer)
                            LierSyncState.addLog("${tr(l, "Сопряжено устройство", "Paired device")}: ${ev.peer.deviceName}")
                        }
                        is MobileEvent.Error -> {
                            LierSyncState.addLog("${tr(LierSyncState.language.value, "Ошибка", "Error")}: ${ev.message}")
                        }
                    }
                }

                val folders = LierSyncState.sharedFolders.value
                if (folders.isNotEmpty() && tick % 3 == 0) {
                    checkFolderChanges(folders)
                }
                if (folders.any { it.autoSync } && tick % 4 == 0) {
                    for (folder in folders) {
                        if (folder.autoSync) {
                            val res = runFolderSyncWithSaf(applicationContext, folder)
                            val obj = runCatching { JSONObject(res) }.getOrNull()
                            if (obj?.optBoolean("ok") == true) {
                                val chunks = obj.optInt("chunks", 0)
                                if (chunks > 0) {
                                    val l = LierSyncState.language.value
                                    LierSyncState.addOrUpdateFolder(
                                        folder.copy(
                                            status = "${tr(l, "Авто-синхр.", "Auto-synced")} ($chunks ${tr(l, "бл.", "chunks")})"
                                        )
                                    )
                                    LierSyncState.addLog(
                                        "[${folder.id}] ${tr(l, "Автосинхронизация", "Auto-sync")}: $chunks ${tr(l, "блоков", "chunks")}"
                                    )
                                    folderSnapshots[folder.id] = snapshotLocalDir(File(folder.dirPath))
                                }
                            }
                        }
                    }
                }
                tick++
                delay(if (activeUi) 1000L else 3500L)
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_PULL_CLIPBOARD -> pullClipboardInBackground()
            ACTION_PUSH_CLIPBOARD -> pushClipboardInBackground()
        }
        return START_STICKY
    }

    override fun onDestroy() {
        clipboardManager?.removePrimaryClipChangedListener(clipListener)
        BluetoothSyncManager.stopServer()
        updateMulticastLock(false)
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun pushClipboardInBackground() {
        val l = LierSyncState.language.value
        val text = readPrimaryClipboardText()
        if (text.isNullOrEmpty()) {
            showToast(tr(l, "Буфер пуст или заблокирован системой", "Clipboard is empty or restricted"))
            return
        }
        pushTextManual(this, text)
    }

    private fun pullClipboardInBackground() {
        val l = LierSyncState.language.value
        val targets = activeTargetAddrs()
        if (targets.isEmpty()) {
            showToast(tr(l, "Нет активного устройства для получения буфера", "No active peer to pull clipboard from"))
            return
        }
        scope.launch {
            var pulled = false
            var lastError = ""
            for (addr in targets) {
                val raw = LierSyncNative.pullClipboardJson(addr)
                val obj = runCatching { JSONObject(raw) }.getOrNull()
                if (obj?.optBoolean("ok") == true) {
                    val kind = obj.optString("kind", "text")
                    val sender = obj.optString("sender_name", addr)
                    if (kind == "image") {
                        val pngPath = obj.optString("png_path")
                        val w = obj.optInt("width", 0)
                        val h = obj.optInt("height", 0)
                        if (pngPath.isNotBlank()) {
                            suppressNextClipChange = true
                            mainHandler.post {
                                writePrimaryClipboardImageFile(pngPath)
                                Toast.makeText(
                                    this@LierSyncService,
                                    "${tr(l, "Картинка скопирована от", "Image copied from")} $sender",
                                    Toast.LENGTH_SHORT
                                ).show()
                            }
                            LierSyncState.addClipboardRecord("Получено", sender, "[${tr(l, "Картинка", "Image")} ${w}x${h}]")
                            LierSyncState.addLog("${tr(l, "Картинка получена от", "Image pulled from")} $sender (${w}x${h})")
                            pulled = true
                            break
                        }
                    } else {
                        val text = obj.optString("text")
                        if (text.isNotEmpty()) {
                            suppressNextClipChange = true
                            mainHandler.post {
                                writePrimaryClipboardText(text)
                                Toast.makeText(
                                    this@LierSyncService,
                                    "${tr(l, "Буфер скопирован от", "Clipboard copied from")} $sender",
                                    Toast.LENGTH_SHORT
                                ).show()
                            }
                            LierSyncState.addClipboardRecord("Получено", sender, text.take(80))
                            LierSyncState.addLog("${tr(l, "Буфер получен от", "Clipboard pulled from")} $sender")
                            pulled = true
                            break
                        }
                    }
                } else {
                    lastError = obj?.optString("error") ?: raw
                }
            }
            if (!pulled && lastError.isNotEmpty()) {
                LierSyncState.addLog("${tr(l, "Не удалось получить буфер", "Failed to pull clipboard")}: $lastError")
                showToast(lastError)
            } else if (!pulled) {
                showToast(tr(l, "Буфер на удалённом устройстве пуст", "Remote clipboard is empty"))
            }
        }
    }

    private fun showToast(message: String) {
        mainHandler.post {
            Toast.makeText(this@LierSyncService, message, Toast.LENGTH_SHORT).show()
        }
    }

    private fun snapshotLocalDir(root: File): Map<String, Pair<Long, Long>> {
        val map = linkedMapOf<String, Pair<Long, Long>>()
        if (!root.exists()) return map
        root.walkTopDown().filter { it.isFile }.forEach { f ->
            val rel = f.relativeTo(root).path.replace('\\', '/')
            if (!rel.contains(".sync-conflict-") && !rel.startsWith(".liersync")) {
                map[rel] = f.length() to (f.lastModified() / 1000L)
            }
        }
        return map
    }

    private fun checkFolderChanges(folders: List<MobileSharedFolder>) {
        val l = LierSyncState.language.value
        for (folder in folders) {
            val safUri = folder.safTreeUri?.let { runCatching { Uri.parse(it) }.getOrNull() }
            val localDir = File(folder.dirPath)
            if (safUri != null && folder.directionCode != "receive_only") {
                mirrorSafToLocal(applicationContext, safUri, localDir)
            }
            val curr = snapshotLocalDir(localDir)
            val prev = folderSnapshots[folder.id]
            if (prev == null) {
                folderSnapshots[folder.id] = curr
                continue
            }
            if (curr == prev) continue
            val added = curr.keys.filter { it !in prev }
            val modified = curr.keys.filter { it in prev && curr[it] != prev[it] }
            val removed = prev.keys.filter { it !in curr }
            folderSnapshots[folder.id] = curr

            val parts = buildList {
                if (added.isNotEmpty()) add("+${added.size} ${tr(l, "добавл.", "added")}")
                if (modified.isNotEmpty()) add("~${modified.size} ${tr(l, "измен.", "modified")}")
                if (removed.isNotEmpty()) add("-${removed.size} ${tr(l, "удал.", "deleted")}")
            }
            if (parts.isEmpty()) continue
            val sample = (removed + modified + added).firstOrNull().orEmpty()
            val summary = if (sample.isNotEmpty()) "${parts.joinToString(", ")} ($sample)" else parts.joinToString(", ")

            LierSyncState.addOrUpdateFolder(
                folder.copy(status = "${tr(l, "Изменено", "Changed")}: $summary")
            )
            LierSyncState.addLog("[${folder.id}] ${tr(l, "Обнаружены изменения", "Folder changed")}: $summary")
            postFolderAlertNotification(folder.id, summary)
        }
    }

    private fun postFolderAlertNotification(folderId: String, summary: String) {
        val l = LierSyncState.language.value
        val alertChannelId = "liersync_alerts"
        val mgr = getSystemService(NotificationManager::class.java) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            mgr.createNotificationChannel(
                NotificationChannel(
                    alertChannelId,
                    tr(l, "Изменения в папках LierSync", "LierSync Folder Changes"),
                    NotificationManager.IMPORTANCE_DEFAULT
                )
            )
        }
        val openAppIntent = PendingIntent.getActivity(
            this,
            105,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, alertChannelId)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        val notif = builder
            .setContentTitle("${tr(l, "Папка", "Folder")} \"$folderId\"")
            .setContentText(summary)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setAutoCancel(true)
            .setContentIntent(openAppIntent)
            .build()
        mgr.notify(200 + (folderId.hashCode() and 0x7F), notif)
    }

    private fun exportToPublicDownloads(
        srcPath: String,
        fileName: String,
        relPath: String?
    ): Pair<String, String?> {
        val srcFile = File(srcPath)
        if (!srcFile.exists()) return "Download/LierSync/$fileName" to null

        val subDir = relPath
            ?.replace('\\', '/')
            ?.substringBeforeLast('/', "")
            ?.trim('/')
            .orEmpty()

        val relativeDownloadPath = if (subDir.isNotEmpty()) {
            "${Environment.DIRECTORY_DOWNLOADS}/LierSync/$subDir"
        } else {
            "${Environment.DIRECTORY_DOWNLOADS}/LierSync"
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val savedUri = runCatching {
                val ext = fileName.substringAfterLast('.', "").lowercase(Locale.ROOT)
                val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)
                    ?: "application/octet-stream"
                val values = ContentValues().apply {
                    put(MediaStore.Downloads.DISPLAY_NAME, fileName)
                    put(MediaStore.Downloads.MIME_TYPE, mime)
                    put(MediaStore.Downloads.RELATIVE_PATH, relativeDownloadPath)
                    put(MediaStore.Downloads.IS_PENDING, 1)
                }
                val resolver = contentResolver
                val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                    ?: return@runCatching null
                resolver.openOutputStream(uri)?.use { out ->
                    srcFile.inputStream().use { input -> input.copyTo(out) }
                }
                values.clear()
                values.put(MediaStore.Downloads.IS_PENDING, 0)
                resolver.update(uri, values, null, null)
                srcFile.delete()
                uri
            }.getOrNull()

            if (savedUri != null) {
                return "$relativeDownloadPath/$fileName" to savedUri.toString()
            }
        }

        @Suppress("DEPRECATION")
        val publicDir = File(
            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
            if (subDir.isNotEmpty()) "LierSync/$subDir" else "LierSync"
        ).apply { mkdirs() }
        val destFile = uniqueTargetFile(publicDir, fileName)
        runCatching {
            srcFile.copyTo(destFile, overwrite = true)
            srcFile.delete()
        }
        return "Download/LierSync/${if (subDir.isNotEmpty()) "$subDir/" else ""}${destFile.name}" to Uri.fromFile(destFile).toString()
    }

    private fun uniqueTargetFile(dir: File, name: String): File {
        val candidate = File(dir, name)
        if (!candidate.exists()) return candidate
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val ext = if (dot > 0) name.substring(dot) else ""
        for (i in 1..999) {
            val f = File(dir, "${stem}_$i$ext")
            if (!f.exists()) return f
        }
        return candidate
    }

    private fun readPrimaryClipboardText(): String? {
        val clip = clipboardManager?.primaryClip ?: return null
        if (clip.itemCount == 0) return null
        return clip.getItemAt(0).coerceToText(this)?.toString()?.takeIf { it.isNotEmpty() }
    }

    private fun readPrimaryClipboardImagePng(): Triple<Int, Int, ByteArray>? {
        return readClipboardImagePngFromContext(this, clipboardManager)
    }

    private fun writePrimaryClipboardText(text: String) {
        val clip = ClipData.newPlainText("LierSync", text)
        clipboardManager?.setPrimaryClip(clip)
    }

    private fun writePrimaryClipboardImageFile(pngPath: String) {
        runCatching {
            val file = File(pngPath)
            if (!file.exists()) return
            val uri = FileProvider.getUriForFile(this, "${packageName}.fileprovider", file)
            val clip = ClipData.newUri(contentResolver, "LierSync Image", uri)
            clipboardManager?.setPrimaryClip(clip)
        }
    }

    private fun detectLanIpv4Addresses(): List<String> = runCatching {
        val result = mutableListOf<String>()
        val ifaces = java.net.NetworkInterface.getNetworkInterfaces() ?: return@runCatching emptyList()
        for (iface in ifaces) {
            if (!iface.isUp || iface.isLoopback) continue
            val name = iface.name.lowercase(Locale.ROOT)
            if (name.startsWith("rmnet") || name.startsWith("ccmni") || name.startsWith("tun")) continue
            for (addr in iface.inetAddresses) {
                if (addr is java.net.Inet4Address && !addr.isLoopbackAddress) {
                    val host = addr.hostAddress ?: continue
                    if (host.startsWith("192.168.")) {
                        result.add(0, host)
                    } else if (addr.isSiteLocalAddress) {
                        result.add(host)
                    }
                }
            }
        }
        result.distinct()
    }.getOrDefault(emptyList())

    private fun updateMulticastLock(needLock: Boolean) {
        runCatching {
            if (needLock) {
                if (multicastLock == null) {
                    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
                    multicastLock = wifi?.createMulticastLock("LierSyncMulticast")?.apply {
                        setReferenceCounted(false)
                    }
                }
                if (multicastLock?.isHeld == false) {
                    multicastLock?.acquire()
                }
            } else {
                if (multicastLock?.isHeld == true) {
                    multicastLock?.release()
                }
            }
        }
    }

    private fun startForegroundCompat() {
        val channelId = "liersync_service"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val mgr = getSystemService(NotificationManager::class.java)
            mgr.createNotificationChannel(
                NotificationChannel(channelId, "LierSync", NotificationManager.IMPORTANCE_LOW)
            )
        }
        val openAppIntent = PendingIntent.getActivity(
            this,
            100,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val pushClipIntent = PendingIntent.getActivity(
            this,
            102,
            Intent(this, ClipboardSyncActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val pullClipIntent = PendingIntent.getService(
            this,
            103,
            Intent(this, LierSyncService::class.java).apply {
                action = ACTION_PULL_CLIPBOARD
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, channelId)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }

        val l = LierSyncState.language.value
        val notification = builder
            .setContentTitle(tr(l, "LierSync активен", "LierSync active"))
            .setContentText(tr(l, "Буфер обмена и приём файлов в фоне", "Clipboard and file receiver in background"))
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setLargeIcon(BitmapFactory.decodeResource(resources, org.larevole.liersync.R.drawable.ic_logo))
            .setContentIntent(openAppIntent)
            .addAction(
                Notification.Action.Builder(null, tr(l, "Отправить буфер", "Send clipboard"), pushClipIntent).build()
            )
            .addAction(
                Notification.Action.Builder(null, tr(l, "Получить буфер", "Pull clipboard"), pullClipIntent).build()
            )
            .build()

        startForeground(101, notification)
    }

    companion object {
        const val ACTION_PULL_CLIPBOARD = "org.larevole.liersync.ACTION_PULL_CLIPBOARD"
        const val ACTION_PUSH_CLIPBOARD = "org.larevole.liersync.ACTION_PUSH_CLIPBOARD"

        fun activeTargetAddrs(): List<String> {
            val discovered = LierSyncState.peers.value.map { it.addr }
            return discovered.ifEmpty {
                listOfNotNull(LierSyncState.targetAddr.value.trim().takeIf { it.isNotEmpty() })
            }
        }

        fun sendBatchAsync(paths: List<String>, targetAddr: String) {
            val l = LierSyncState.language.value
            CoroutineScope(Dispatchers.IO).launch {
                val jsonArr = JSONArray(paths).toString()
                val res = LierSyncNative.sendBatchJson(targetAddr, jsonArr)
                val obj = runCatching { JSONObject(res) }.getOrNull()
                if (obj?.optBoolean("ok") == true) {
                    val count = obj.optInt("sent_files", paths.size)
                    for (p in paths) {
                        val f = File(p)
                        LierSyncState.addTransferRecord(
                            fileName = f.name,
                            peer = targetAddr,
                            displayPath = p,
                            contentUri = null,
                            bytes = f.length(),
                            isSent = true
                        )
                    }
                    LierSyncState.addLog("${tr(l, "Отправлено файлов", "Sent files")}: $count -> $targetAddr")
                } else {
                    LierSyncState.addLog("${tr(l, "Ошибка отправки", "Send error")}: ${obj?.optString("error") ?: res}")
                }
            }
        }

        fun showToastGlobal(context: Context, message: String) {
            Handler(Looper.getMainLooper()).post {
                Toast.makeText(context.applicationContext, message, Toast.LENGTH_SHORT).show()
            }
        }

        fun pushTextToActivePeers(text: String) {
            val targets = activeTargetAddrs()
            if (targets.isEmpty()) return
            CoroutineScope(Dispatchers.IO).launch {
                for (addr in targets) {
                    val res = LierSyncNative.pushClipboardJson(addr, text, false)
                    val obj = runCatching { JSONObject(res) }.getOrNull()
                    if (obj?.optBoolean("ok") == true && obj.optString("skipped").isEmpty()) {
                        LierSyncState.addClipboardRecord("Отправлено", addr, text.take(80))
                    }
                }
            }
        }

        fun pushTextManual(context: Context, text: String, preferredAddr: String? = null) {
            val l = LierSyncState.language.value
            val targets = buildList {
                addAll(activeTargetAddrs())
                val pref = preferredAddr?.trim().orEmpty()
                if (pref.isNotEmpty() && pref !in this) add(pref)
            }
            if (targets.isEmpty()) {
                val msg = tr(l, "Нет активного устройства для отправки буфера", "No active peer for clipboard push")
                LierSyncState.addLog(msg)
                showToastGlobal(context, msg)
                return
            }
            CoroutineScope(Dispatchers.IO).launch {
                var sentAny = false
                var lastErr = ""
                for (addr in targets) {
                    val res = LierSyncNative.pushClipboardJson(addr, text, true)
                    val obj = runCatching { JSONObject(res) }.getOrNull()
                    if (obj?.optBoolean("ok") == true) {
                        LierSyncState.addClipboardRecord("Отправлено", addr, text.take(80))
                        LierSyncState.addLog("${tr(l, "Буфер отправлен на", "Clipboard sent to")} $addr")
                        sentAny = true
                    } else {
                        lastErr = obj?.optString("error") ?: res
                        LierSyncState.addLog("${tr(l, "Ошибка буфера", "Clipboard error")} ($addr): $lastErr")
                    }
                }
                if (sentAny) {
                    showToastGlobal(context, tr(l, "Буфер отправлен", "Clipboard sent"))
                } else if (lastErr.isNotEmpty()) {
                    showToastGlobal(context, lastErr)
                }
            }
        }

        /**
         * Resolves a document tree URI picked via OpenDocumentTree to a direct storage path if writable,
         * otherwise returns a local mirror path paired with the persistable SAF URI string.
         */
        fun resolvePickedFolder(context: Context, treeUri: Uri, folderId: String): Pair<String, String?> {
            val docId = runCatching { DocumentsContract.getTreeDocumentId(treeUri) }.getOrNull().orEmpty()
            if (docId.startsWith("primary:")) {
                val rel = docId.removePrefix("primary:").trimStart('/')
                @Suppress("DEPRECATION")
                val direct = if (rel.isEmpty()) {
                    Environment.getExternalStorageDirectory()
                } else {
                    File(Environment.getExternalStorageDirectory(), rel)
                }
                val canDirect = (direct.exists() || direct.mkdirs()) && direct.canRead() && direct.canWrite()
                if (canDirect) {
                    return direct.absolutePath to null
                }
            }
            val mirrorDir = File(context.filesDir, "LierSync_SAF/${folderId.ifBlank { "default" }}").apply {
                mkdirs()
            }
            if (treeUri.toString().isNotBlank()) {
                mirrorSafToLocal(context, treeUri, mirrorDir)
            }
            return mirrorDir.absolutePath to treeUri.toString()
        }

        /**
         * Syncs a shared folder through Rust, mirroring to/from its SAF tree URI before and after when present.
         */
        fun runFolderSyncWithSaf(context: Context, folder: MobileSharedFolder): String {
            val safUri = folder.safTreeUri?.let { runCatching { Uri.parse(it) }.getOrNull() }
            val localDir = File(folder.dirPath).apply { mkdirs() }
            if (safUri != null && folder.directionCode != "receive_only") {
                mirrorSafToLocal(context, safUri, localDir)
            }
            val res = LierSyncNative.syncFolderJson(
                folder.remoteAddr,
                folder.id,
                folder.dirPath,
                folder.directionCode
            )
            val obj = runCatching { JSONObject(res) }.getOrNull()
            if (safUri != null && obj?.optBoolean("ok") == true && folder.directionCode != "send_only") {
                mirrorLocalToSaf(context, localDir, safUri)
            }
            return res
        }

        fun runFolderCompareWithSaf(context: Context, folder: MobileSharedFolder): String {
            val safUri = folder.safTreeUri?.let { runCatching { Uri.parse(it) }.getOrNull() }
            val localDir = File(folder.dirPath).apply { mkdirs() }
            if (safUri != null && folder.directionCode != "receive_only") {
                mirrorSafToLocal(context, safUri, localDir)
            }
            return LierSyncNative.compareFolderJson(folder.remoteAddr, folder.id, folder.dirPath)
        }

        private fun mirrorSafToLocal(context: Context, treeUri: Uri, localRoot: File) {
            runCatching {
                val rootDocId = DocumentsContract.getTreeDocumentId(treeUri)
                mirrorSafDirToLocal(context, treeUri, rootDocId, localRoot)
            }
        }

        private fun mirrorSafDirToLocal(
            context: Context,
            treeUri: Uri,
            parentDocId: String,
            localDir: File
        ) {
            localDir.mkdirs()
            val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId)
            val projection = arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
                DocumentsContract.Document.COLUMN_LAST_MODIFIED,
                DocumentsContract.Document.COLUMN_SIZE
            )
            context.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
                val idIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                val modIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
                val sizeIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
                while (cursor.moveToNext()) {
                    val docId = if (idIdx >= 0) cursor.getString(idIdx) else continue
                    val name = if (nameIdx >= 0) cursor.getString(nameIdx) else continue
                    if (name.startsWith(".liersync") || name.isBlank()) continue
                    val mime = if (mimeIdx >= 0) cursor.getString(mimeIdx) else ""
                    if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                        mirrorSafDirToLocal(context, treeUri, docId, File(localDir, name))
                    } else {
                        val safMod = if (modIdx >= 0) cursor.getLong(modIdx) else 0L
                        val safSize = if (sizeIdx >= 0) cursor.getLong(sizeIdx) else -1L
                        val target = File(localDir, name)
                        val needsCopy = !target.exists() ||
                            (safMod > target.lastModified() + 1500L) ||
                            (safSize >= 0 && safSize != target.length() && safMod >= target.lastModified() - 1500L)
                        if (needsCopy) {
                            val fileUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId)
                            context.contentResolver.openInputStream(fileUri)?.use { input ->
                                target.outputStream().use { out -> input.copyTo(out) }
                            }
                            if (safMod > 0L) {
                                target.setLastModified(safMod)
                            }
                        }
                    }
                }
            }
        }

        private data class SafChild(
            val docId: String,
            val uri: Uri,
            val isDir: Boolean,
            val lastModified: Long,
            val size: Long
        )

        private fun listSafChildren(
            context: Context,
            treeUri: Uri,
            parentDocId: String
        ): Map<String, SafChild> {
            val map = linkedMapOf<String, SafChild>()
            val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId)
            val projection = arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
                DocumentsContract.Document.COLUMN_LAST_MODIFIED,
                DocumentsContract.Document.COLUMN_SIZE
            )
            context.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
                val idIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                val modIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
                val sizeIdx = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
                while (cursor.moveToNext()) {
                    val docId = if (idIdx >= 0) cursor.getString(idIdx) else continue
                    val name = if (nameIdx >= 0) cursor.getString(nameIdx) else continue
                    val mime = if (mimeIdx >= 0) cursor.getString(mimeIdx) else ""
                    val mod = if (modIdx >= 0) cursor.getLong(modIdx) else 0L
                    val size = if (sizeIdx >= 0) cursor.getLong(sizeIdx) else 0L
                    map[name] = SafChild(
                        docId = docId,
                        uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId),
                        isDir = mime == DocumentsContract.Document.MIME_TYPE_DIR,
                        lastModified = mod,
                        size = size
                    )
                }
            }
            return map
        }

        private fun mirrorLocalToSaf(context: Context, localRoot: File, treeUri: Uri) {
            runCatching {
                val rootDocId = DocumentsContract.getTreeDocumentId(treeUri)
                mirrorLocalDirToSaf(context, localRoot, treeUri, rootDocId)
            }
        }

        private fun mirrorLocalDirToSaf(
            context: Context,
            localDir: File,
            treeUri: Uri,
            parentDocId: String
        ) {
            val files = localDir.listFiles() ?: return
            val existing = listSafChildren(context, treeUri, parentDocId)
            val parentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, parentDocId)
            val resolver = context.contentResolver
            for (entry in files) {
                if (entry.name.startsWith(".liersync")) continue
                val child = existing[entry.name]
                if (entry.isDirectory) {
                    val subDocId = if (child != null && child.isDir) {
                        child.docId
                    } else {
                        val created = DocumentsContract.createDocument(
                            resolver,
                            parentUri,
                            DocumentsContract.Document.MIME_TYPE_DIR,
                            entry.name
                        ) ?: continue
                        DocumentsContract.getDocumentId(created)
                    }
                    mirrorLocalDirToSaf(context, entry, treeUri, subDocId)
                } else if (entry.isFile) {
                    val needsWrite = child == null ||
                        child.isDir ||
                        entry.lastModified() > child.lastModified + 1500L ||
                        (entry.length() != child.size && entry.lastModified() >= child.lastModified)
                    if (needsWrite) {
                        val destUri = if (child != null && !child.isDir) {
                            child.uri
                        } else {
                            val ext = entry.extension.lowercase(Locale.ROOT)
                            val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)
                                ?: "application/octet-stream"
                            DocumentsContract.createDocument(resolver, parentUri, mime, entry.name)
                                ?: continue
                        }
                        resolver.openOutputStream(destUri, "wt")?.use { out ->
                            entry.inputStream().use { input -> input.copyTo(out) }
                        }
                    }
                }
            }
        }

        fun pullClipboardAsync(context: Context) {
            val intent = Intent(context, LierSyncService::class.java).apply {
                action = ACTION_PULL_CLIPBOARD
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun checkClipboardIfAuto(context: Context) {
            if (!LierSyncState.autoClipboard.value) return
            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            readClipboardImagePngFromContext(context, cm)?.let { (w, h, png) ->
                pushImageToActivePeers(w, h, png)
                return
            }
            val text = cm?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)
                ?.coerceToText(context)?.toString()
            if (!text.isNullOrEmpty()) {
                pushTextToActivePeers(text)
            }
        }

        fun pushImageToActivePeers(width: Int, height: Int, pngBytes: ByteArray) {
            val targets = activeTargetAddrs()
            if (targets.isEmpty()) return
            CoroutineScope(Dispatchers.IO).launch {
                for (addr in targets) {
                    val res = LierSyncNative.pushClipboardImageBytes(addr, width, height, pngBytes, false)
                    val obj = runCatching { JSONObject(res) }.getOrNull()
                    if (obj?.optBoolean("ok") == true && obj.optString("skipped").isEmpty()) {
                        LierSyncState.addClipboardRecord("Отправлено", addr, "[Image ${width}x${height}]")
                    }
                }
            }
        }

        fun pushImageManual(
            context: Context,
            width: Int,
            height: Int,
            pngBytes: ByteArray,
            preferredAddr: String? = null
        ) {
            val l = LierSyncState.language.value
            val targets = buildList {
                addAll(activeTargetAddrs())
                val pref = preferredAddr?.trim().orEmpty()
                if (pref.isNotEmpty() && pref !in this) add(pref)
            }
            if (targets.isEmpty()) {
                val msg = tr(l, "Нет активного устройства для отправки картинки", "No active peer for clipboard image")
                LierSyncState.addLog(msg)
                showToastGlobal(context, msg)
                return
            }
            CoroutineScope(Dispatchers.IO).launch {
                var sentAny = false
                var lastErr = ""
                for (addr in targets) {
                    val res = LierSyncNative.pushClipboardImageBytes(addr, width, height, pngBytes, true)
                    val obj = runCatching { JSONObject(res) }.getOrNull()
                    if (obj?.optBoolean("ok") == true) {
                        LierSyncState.addClipboardRecord("Отправлено", addr, "[Image ${width}x${height}]")
                        LierSyncState.addLog("${tr(l, "Картинка буфера отправлена на", "Clipboard image sent to")} $addr (${width}x${height})")
                        sentAny = true
                    } else {
                        lastErr = obj?.optString("error") ?: res
                    }
                }
                if (sentAny) {
                    showToastGlobal(context, tr(l, "Картинка отправлена", "Image sent"))
                } else if (lastErr.isNotEmpty()) {
                    showToastGlobal(context, lastErr)
                }
            }
        }

        fun readClipboardImagePngFromContext(
            context: Context,
            cm: ClipboardManager?
        ): Triple<Int, Int, ByteArray>? = runCatching {
            val clip = cm?.primaryClip ?: return@runCatching null
            if (clip.itemCount == 0) return@runCatching null
            val uri = clip.getItemAt(0).uri ?: return@runCatching null
            val mime = context.contentResolver.getType(uri).orEmpty()
            if (!mime.startsWith("image/") && !clip.description.hasMimeType("image/*")) {
                return@runCatching null
            }
            val decoded = context.contentResolver.openInputStream(uri)?.use { input ->
                BitmapFactory.decodeStream(input)
            } ?: return@runCatching null
            val maxDim = 1920
            val bmp = if (decoded.width > maxDim || decoded.height > maxDim) {
                val ratio = minOf(maxDim.toFloat() / decoded.width, maxDim.toFloat() / decoded.height)
                Bitmap.createScaledBitmap(
                    decoded,
                    (decoded.width * ratio).toInt().coerceAtLeast(1),
                    (decoded.height * ratio).toInt().coerceAtLeast(1),
                    true
                )
            } else {
                decoded
            }
            val baos = ByteArrayOutputStream()
            bmp.compress(Bitmap.CompressFormat.PNG, 100, baos)
            Triple(bmp.width, bmp.height, baos.toByteArray())
        }.getOrNull()

        fun pushClipboardNow(context: Context, targetAddr: String) {
            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            readClipboardImagePngFromContext(context, cm)?.let { (w, h, png) ->
                pushImageManual(context, w, h, png, targetAddr)
                return
            }
            val text = cm?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)
                ?.coerceToText(context)?.toString()
            if (text.isNullOrEmpty()) {
                val msg = tr(LierSyncState.language.value, "Буфер обмена пуст", "Clipboard is empty")
                LierSyncState.addLog(msg)
                showToastGlobal(context, msg)
                return
            }
            pushTextManual(context, text, targetAddr)
        }

        fun sendClipboardViaBluetooth(context: Context, mac: String) {
            val l = LierSyncState.language.value
            if (mac.isBlank()) {
                showToastGlobal(context, tr(l, "Выберите Bluetooth-устройство", "Select a Bluetooth device"))
                return
            }
            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            val img = readClipboardImagePngFromContext(context, cm)
            val text = cm?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)
                ?.coerceToText(context)?.toString()
            CoroutineScope(Dispatchers.IO).launch {
                val res = if (img != null) {
                    BluetoothSyncManager.sendClipboardImage(context, mac, img.first, img.second, img.third)
                } else if (!text.isNullOrEmpty()) {
                    BluetoothSyncManager.sendClipboardText(context, mac, text)
                } else {
                    Result.failure(IllegalStateException(tr(l, "Буфер пуст", "Clipboard is empty")))
                }
                res.onSuccess {
                    LierSyncState.addLog("${tr(l, "Отправлено по Bluetooth", "Sent via Bluetooth")} -> $mac")
                    showToastGlobal(context, tr(l, "Отправлено по Bluetooth", "Sent via Bluetooth"))
                }.onFailure { err ->
                    LierSyncState.addLog("BT: ${err.message}")
                    showToastGlobal(context, err.message ?: "Bluetooth error")
                }
            }
        }

        fun sendFilesViaBluetooth(context: Context, mac: String, paths: List<String>) {
            val l = LierSyncState.language.value
            if (mac.isBlank() || paths.isEmpty()) {
                showToastGlobal(context, tr(l, "Выберите устройство Bluetooth и файлы", "Pick Bluetooth device & files"))
                return
            }
            CoroutineScope(Dispatchers.IO).launch {
                BluetoothSyncManager.sendFiles(context, mac, paths)
                    .onSuccess { count ->
                        LierSyncState.addLog("${tr(l, "Файлов по Bluetooth", "Bluetooth files sent")}: $count -> $mac")
                        showToastGlobal(context, "${tr(l, "Отправлено", "Sent")}: $count")
                    }
                    .onFailure { err ->
                        LierSyncState.addLog("BT: ${err.message}")
                        showToastGlobal(context, err.message ?: "Bluetooth error")
                    }
            }
        }
    }
}
