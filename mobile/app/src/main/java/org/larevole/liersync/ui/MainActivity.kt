// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.ui

import android.Manifest
import android.app.Activity
import android.app.DownloadManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONObject
import org.larevole.liersync.nativebridge.LierSyncNative
import org.larevole.liersync.nativebridge.NodeInfo
import org.larevole.liersync.service.BluetoothSyncManager
import org.larevole.liersync.service.LierSyncService
import org.larevole.liersync.service.LierSyncState
import org.larevole.liersync.service.MobileSharedFolder
import org.larevole.liersync.service.tr
import java.io.File

private val BgBase = Color(0xFF121316)
private val BgSurface = Color(0xFF1A1B20)
private val BgElevated = Color(0xFF24262E)
private val BorderSubtle = Color(0xFF333642)
private val AccentTeal = Color(0xFF39C5BB)
private val AccentRed = Color(0xFF8B0000)
private val TextMain = Color(0xFFE0E2EC)
private val TextMuted = Color(0xFF9EA2B0)

private val CardBorder = BorderStroke(1.dp, BorderSubtle)
private val TealBorder = BorderStroke(1.dp, AccentTeal)
private val CardShape = RoundedCornerShape(10.dp)

private val LierThemeColors = darkColorScheme(
    primary = AccentTeal,
    onPrimary = BgBase,
    secondary = AccentRed,
    onSecondary = TextMain,
    background = BgBase,
    surface = BgSurface,
    surfaceVariant = BgElevated,
    onBackground = TextMain,
    onSurface = TextMain,
    onSurfaceVariant = TextMuted,
    outline = BorderSubtle,
    error = Color(0xFFA81313)
)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val perms = buildList {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                add(Manifest.permission.BLUETOOTH_CONNECT)
                add(Manifest.permission.BLUETOOTH_SCAN)
            }
        }
        if (perms.isNotEmpty()) {
            registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {}
                .launch(perms.toTypedArray())
        }

        val svcIntent = Intent(this, LierSyncService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(svcIntent)
        } else {
            startService(svcIntent)
        }

        setContent {
            MaterialTheme(colorScheme = LierThemeColors) {
                LierSyncMobileApp()
            }
        }
    }

    override fun onStart() {
        super.onStart()
        LierSyncState.isUiVisible = true
    }

    override fun onStop() {
        LierSyncState.isUiVisible = false
        super.onStop()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            LierSyncService.checkClipboardIfAuto(this)
        }
    }
}

private fun formatMyAddress(nodeInfo: NodeInfo?, lang: String = LierSyncState.language.value): String {
    if (nodeInfo == null) return tr(lang, "Определение...", "Detecting...")
    val ip = nodeInfo.localIps.firstOrNull() ?: "127.0.0.1"
    return "$ip:${nodeInfo.p2pPort}"
}

private fun copyToPhoneClipboard(context: Context, text: String, label: String) {
    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
    cm?.setPrimaryClip(ClipData.newPlainText(label, text))
    LierSyncState.addLog("${tr(LierSyncState.language.value, "Скопировано", "Copied")}: $text")
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun LierSyncMobileApp() {
    val context = LocalContext.current
    var selectedTab by remember { mutableIntStateOf(0) }
    var showSettings by remember { mutableStateOf(false) }
    var showAbout by remember { mutableStateOf(false) }
    val nodeInfo by LierSyncState.nodeInfo.collectAsState()
    val peers by LierSyncState.peers.collectAsState()
    val lang by LierSyncState.language.collectAsState()
    val autoClip by LierSyncState.autoClipboard.collectAsState()
    val encryptTransport by LierSyncState.encryptTransport.collectAsState()
    val whitelistOnly by LierSyncState.whitelistOnly.collectAsState()
    val pairingPin by LierSyncState.pairingPin.collectAsState()
    val trustedPeers by LierSyncState.trustedPeers.collectAsState()
    val targetAddr by LierSyncState.targetAddr.collectAsState()
    var pairPinInput by remember { mutableStateOf("") }
    var pairStatus by remember { mutableStateOf("") }
    val settingsScope = rememberCoroutineScope()
    val myAddress = formatMyAddress(nodeInfo, lang)

    if (showSettings) {
        AlertDialog(
            onDismissRequest = { showSettings = false },
            containerColor = BgSurface,
            title = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        imageVector = Icons.Rounded.Settings,
                        contentDescription = null,
                        tint = AccentTeal,
                        modifier = Modifier.size(22.dp)
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(tr(lang, "Настройки", "Settings"), color = TextMain)
                }
            },
            text = {
                Column(
                    modifier = Modifier.verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Text(
                        tr(lang, "Язык интерфейса", "Interface Language"),
                        color = TextMain,
                        fontWeight = FontWeight.SemiBold
                    )
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(
                            selected = lang == "ru",
                            onClick = { LierSyncState.setLanguage("ru") },
                            label = { Text("Русский") },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = AccentTeal,
                                selectedLabelColor = BgBase
                            )
                        )
                        FilterChip(
                            selected = lang == "en",
                            onClick = { LierSyncState.setLanguage("en") },
                            label = { Text("English") },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = AccentTeal,
                                selectedLabelColor = BgBase
                            )
                        )
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            tr(lang, "Автосинхронизация буфера", "Auto-sync clipboard"),
                            color = TextMain,
                            modifier = Modifier.weight(1f)
                        )
                        Switch(
                            checked = autoClip,
                            onCheckedChange = { LierSyncState.setAutoClipboard(it) },
                            colors = SwitchDefaults.colors(checkedTrackColor = AccentTeal, checkedThumbColor = BgBase)
                        )
                    }

                    Text(
                        tr(lang, "Безопасность и сопряжение", "Security & Pairing"),
                        color = TextMain,
                        fontWeight = FontWeight.SemiBold
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            tr(lang, "Шифрование (X25519 + ChaCha20)", "Encryption (X25519 + ChaCha20)"),
                            color = TextMain,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.weight(1f)
                        )
                        Switch(
                            checked = encryptTransport,
                            onCheckedChange = { LierSyncState.setEncryptTransport(it) },
                            colors = SwitchDefaults.colors(checkedTrackColor = AccentTeal, checkedThumbColor = BgBase)
                        )
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            tr(lang, "Только белый список", "Trusted whitelist only"),
                            color = TextMain,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.weight(1f)
                        )
                        Switch(
                            checked = whitelistOnly,
                            onCheckedChange = { LierSyncState.setWhitelistOnly(it) },
                            colors = SwitchDefaults.colors(checkedTrackColor = AccentTeal, checkedThumbColor = BgBase)
                        )
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            "${tr(lang, "Мой PIN", "My PIN")}: $pairingPin",
                            color = AccentTeal,
                            fontWeight = FontWeight.Bold
                        )
                        TextButton(onClick = { LierSyncState.rotatePairingPin() }) {
                            Text(tr(lang, "Новый PIN", "Rotate"), color = AccentTeal)
                        }
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        OutlinedTextField(
                            value = pairPinInput,
                            onValueChange = { s -> pairPinInput = s.filter { it.isDigit() }.take(8) },
                            label = { Text(tr(lang, "PIN ПК", "Peer PIN")) },
                            singleLine = true,
                            modifier = Modifier.weight(1f)
                        )
                        Button(
                            onClick = {
                                val pin = pairPinInput.trim()
                                val addr = LierSyncService.activeTargetAddrs().firstOrNull() ?: targetAddr
                                if (pin.isEmpty()) {
                                    pairStatus = tr(lang, "Введите PIN", "Enter PIN")
                                } else {
                                    pairStatus = tr(lang, "Сопряжение...", "Pairing...")
                                    settingsScope.launch(Dispatchers.IO) {
                                        val res = LierSyncNative.pairWithPeerJson(addr, pin)
                                        val obj = runCatching { JSONObject(res) }.getOrNull()
                                        if (obj?.optBoolean("ok") == true) {
                                            val item = org.larevole.liersync.nativebridge.TrustedPeerItem(
                                                deviceId = obj.optString("device_id"),
                                                deviceName = obj.optString("device_name"),
                                                fingerprint = obj.optString("fingerprint"),
                                                addedAt = obj.optString("added_at")
                                            )
                                            LierSyncState.addTrustedPeer(item)
                                            pairStatus = "${tr(lang, "Сопряжено", "Paired")}: ${item.deviceName}"
                                        } else {
                                            pairStatus = "${tr(lang, "Ошибка", "Error")}: ${obj?.optString("error") ?: res}"
                                        }
                                    }
                                }
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                        ) {
                            Text(tr(lang, "Сопрячь", "Pair"))
                        }
                    }
                    if (pairStatus.isNotEmpty()) {
                        Text(pairStatus, style = MaterialTheme.typography.labelSmall, color = AccentTeal)
                    }
                    if (trustedPeers.isNotEmpty()) {
                        Text(
                            tr(lang, "Доверенные устройства:", "Trusted devices:"),
                            style = MaterialTheme.typography.labelMedium,
                            color = TextMuted
                        )
                        trustedPeers.forEach { tp ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Text(
                                    "${tp.deviceName} (${tp.fingerprint})",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = TextMain,
                                    modifier = Modifier.weight(1f)
                                )
                                IconButton(onClick = { LierSyncState.removeTrustedPeer(tp.deviceId) }) {
                                    Icon(LierIcons.Trash, contentDescription = null, tint = AccentRed, modifier = Modifier.size(16.dp))
                                }
                            }
                        }
                    }

                    Text(
                        "${tr(lang, "Папка приёма", "Downloads folder")}: Download/LierSync",
                        style = MaterialTheme.typography.bodySmall,
                        color = TextMuted
                    )
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && !Environment.isExternalStorageManager()) {
                        OutlinedButton(
                            onClick = {
                                val intent = Intent(
                                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                                    Uri.parse("package:${context.packageName}")
                                )
                                runCatching { context.startActivity(intent) }
                            },
                            border = TealBorder,
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Text(
                                tr(
                                    lang,
                                    "Доступ ко всем папкам (опционально)",
                                    "All-files folder access (optional)"
                                ),
                                color = AccentTeal,
                                style = MaterialTheme.typography.labelMedium
                            )
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { showSettings = false }) {
                    Text(tr(lang, "Закрыть", "Close"), color = AccentTeal)
                }
            }
        )
    }

    if (showAbout) {
        AlertDialog(
            onDismissRequest = { showAbout = false },
            containerColor = BgSurface,
            title = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Image(
                        painter = painterResource(org.larevole.liersync.R.drawable.ic_logo),
                        contentDescription = "LierSync",
                        modifier = Modifier
                            .size(36.dp)
                            .clip(RoundedCornerShape(8.dp))
                    )
                    Spacer(Modifier.width(10.dp))
                    Text(tr(lang, "О программе LierSync", "About LierSync"), color = TextMain)
                }
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("${tr(lang, "Версия", "Version")}: 1.1", color = AccentTeal, fontWeight = FontWeight.SemiBold)
                    Text("${tr(lang, "Разработчик", "Developer")}: LaRevole (LaRCorp)", color = TextMain)
                    Text("${tr(lang, "Лицензия", "License")}: GNU GPLv3 (GPL-3.0-only)", color = TextMuted)
                    Text("${tr(lang, "Сохранение файлов", "Saved files")}: Download/LierSync", color = TextMuted)
                    nodeInfo?.let {
                        Text("${tr(lang, "Узел", "Node")}: ${it.deviceName} (${it.fingerprint})", color = TextMuted)
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { showAbout = false }) {
                    Text(tr(lang, "Закрыть", "Close"), color = AccentTeal)
                }
            }
        )
    }

    Scaffold(
        modifier = Modifier
            .fillMaxSize()
            .background(BgBase)
            .statusBarsPadding()
            .navigationBarsPadding(),
        containerColor = BgBase,
        topBar = {
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 6.dp),
                colors = CardDefaults.cardColors(containerColor = BgSurface),
                border = CardBorder,
                shape = CardShape
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Image(
                                painter = painterResource(org.larevole.liersync.R.drawable.ic_logo),
                                contentDescription = "LierSync",
                                modifier = Modifier
                                    .size(26.dp)
                                    .clip(RoundedCornerShape(6.dp))
                            )
                            Spacer(Modifier.width(8.dp))
                            Text(
                                "LierSync",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.Bold,
                                color = TextMain
                            )
                            Spacer(Modifier.width(6.dp))
                            Surface(
                                color = BgElevated,
                                shape = RoundedCornerShape(4.dp),
                                border = BorderStroke(1.dp, AccentTeal.copy(alpha = 0.35f))
                            ) {
                                Text(
                                    "v1.2",
                                    color = AccentTeal,
                                    style = MaterialTheme.typography.labelSmall,
                                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp)
                                )
                            }
                        }
                        Spacer(Modifier.height(2.dp))
                        Text(
                            "${tr(lang, "Мой адрес", "My address")}: $myAddress · ${tr(lang, "Пиров", "Peers")}: ${peers.size}",
                            style = MaterialTheme.typography.bodySmall,
                            color = AccentTeal
                        )
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        IconButton(onClick = { copyToPhoneClipboard(context, myAddress, "LierSync Address") }) {
                            Icon(
                                imageVector = LierIcons.Copy,
                                contentDescription = tr(lang, "Копировать адрес", "Copy address"),
                                tint = TextMain,
                                modifier = Modifier.size(18.dp)
                            )
                        }
                        IconButton(onClick = { showSettings = true }) {
                            Icon(
                                imageVector = Icons.Rounded.Settings,
                                contentDescription = tr(lang, "Настройки", "Settings"),
                                tint = TextMain,
                                modifier = Modifier.size(19.dp)
                            )
                        }
                        IconButton(onClick = { showAbout = true }) {
                            Icon(
                                imageVector = Icons.Rounded.Info,
                                contentDescription = tr(lang, "О программе", "About"),
                                tint = TextMuted,
                                modifier = Modifier.size(19.dp)
                            )
                        }
                    }
                }
            }
        },
        bottomBar = {
            NavigationBar(containerColor = BgSurface) {
                val itemColors = NavigationBarItemDefaults.colors(
                    selectedIconColor = AccentTeal,
                    selectedTextColor = AccentTeal,
                    indicatorColor = BgElevated,
                    unselectedIconColor = TextMuted,
                    unselectedTextColor = TextMuted
                )
                NavigationBarItem(
                    selected = selectedTab == 0,
                    onClick = { selectedTab = 0 },
                    colors = itemColors,
                    icon = { Icon(LierIcons.Send, contentDescription = tr(lang, "Отправка", "Send"), modifier = Modifier.size(20.dp)) },
                    label = { Text(tr(lang, "Отправка", "Send")) }
                )
                NavigationBarItem(
                    selected = selectedTab == 1,
                    onClick = { selectedTab = 1 },
                    colors = itemColors,
                    icon = { Icon(LierIcons.Sync, contentDescription = tr(lang, "Синхронизация", "Sync"), modifier = Modifier.size(20.dp)) },
                    label = { Text(tr(lang, "Синхр.", "Sync")) }
                )
                NavigationBarItem(
                    selected = selectedTab == 2,
                    onClick = { selectedTab = 2 },
                    colors = itemColors,
                    icon = { Icon(LierIcons.Clipboard, contentDescription = tr(lang, "Буфер", "Clipboard"), modifier = Modifier.size(20.dp)) },
                    label = { Text(tr(lang, "Буфер", "Clipboard")) }
                )
                NavigationBarItem(
                    selected = selectedTab == 3,
                    onClick = { selectedTab = 3 },
                    colors = itemColors,
                    icon = { Icon(LierIcons.Channels, contentDescription = tr(lang, "Каналы", "Channels"), modifier = Modifier.size(20.dp)) },
                    label = { Text(tr(lang, "Каналы", "Channels")) }
                )
            }
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 12.dp)
        ) {
            when (selectedTab) {
                0 -> QuickSendScreen()
                1 -> FolderSyncScreen()
                2 -> ClipboardScreen()
                3 -> ChannelsScreen()
            }
        }
    }
}

@Composable
private fun LierCard(content: @Composable () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = BgSurface),
        border = CardBorder,
        shape = CardShape
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            content()
        }
    }
}

@Composable
fun QuickSendScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val peers by LierSyncState.peers.collectAsState()
    val targetAddr by LierSyncState.targetAddr.collectAsState()
    val transferHistory by LierSyncState.transferHistory.collectAsState()
    val logs by LierSyncState.logs.collectAsState()
    val pendingShare by LierSyncState.pendingShareFiles.collectAsState()
    val lang by LierSyncState.language.collectAsState()
    val sendQueue = remember { mutableStateListOf<String>() }
    var pingStatus by remember { mutableStateOf("") }

    LaunchedEffect(pendingShare) {
        if (pendingShare.isNotEmpty()) {
            for (p in LierSyncState.consumePendingShareFiles()) {
                if (!sendQueue.contains(p)) sendQueue.add(p)
            }
        }
    }

    val multiFilePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments()
    ) { uris ->
        for (uri in uris) {
            copyUriToStaging(context, uri)?.let { path ->
                if (!sendQueue.contains(path)) sendQueue.add(path)
            }
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item(key = "qs_recipient") {
            LierCard {
                Text(tr(lang, "1. Получатель", "1. Recipient"), fontWeight = FontWeight.SemiBold, color = TextMain)
                if (peers.isEmpty()) {
                    Text(
                        tr(
                            lang,
                            "Устройства в сети появятся автоматически, либо введите адрес с ПК.",
                            "Devices on your network appear automatically, or enter the PC address."
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = TextMuted
                    )
                } else {
                    peers.forEach { peer ->
                        val isSelected = peer.addr == targetAddr
                        OutlinedButton(
                            onClick = { LierSyncState.setTargetAddr(peer.addr) },
                            modifier = Modifier.fillMaxWidth(),
                            border = if (isSelected) TealBorder else CardBorder
                        ) {
                            if (isSelected) {
                                Icon(
                                    Icons.Rounded.CheckCircle,
                                    contentDescription = null,
                                    tint = AccentTeal,
                                    modifier = Modifier.size(16.dp)
                                )
                                Spacer(Modifier.width(6.dp))
                            }
                            Text(
                                "${peer.deviceName} (${peer.os}) — ${peer.addr}",
                                color = if (isSelected) AccentTeal else TextMain
                            )
                        }
                    }
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    OutlinedTextField(
                        value = targetAddr,
                        onValueChange = { LierSyncState.setTargetAddr(it) },
                        label = { Text(tr(lang, "IP:порт получателя", "Recipient IP:port")) },
                        singleLine = true,
                        modifier = Modifier.weight(1f)
                    )
                    OutlinedButton(onClick = {
                        pingStatus = tr(lang, "Проверка...", "Checking...")
                        scope.launch(Dispatchers.IO) {
                            val raw = LierSyncNative.pingPeerJson(targetAddr)
                            val obj = runCatching { JSONObject(raw) }.getOrNull()
                            pingStatus = if (obj?.optBoolean("ok") == true) {
                                "${obj.optString("device_name")} · ${obj.optLong("latency_ms")} ${tr(lang, "мс", "ms")}"
                            } else {
                                obj?.optString("error") ?: raw
                            }
                        }
                    }) {
                        Icon(Icons.Rounded.Search, contentDescription = null, modifier = Modifier.size(16.dp))
                    }
                }
                if (pingStatus.isNotEmpty()) {
                    Text("${tr(lang, "Связь", "Link")}: $pingStatus", style = MaterialTheme.typography.bodySmall, color = AccentTeal)
                }
            }
        }

        item(key = "qs_files") {
            LierCard {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text(tr(lang, "2. Файлы", "2. Files"), fontWeight = FontWeight.SemiBold, color = TextMain)
                        Text("Download/LierSync", style = MaterialTheme.typography.labelSmall, color = TextMuted)
                    }
                    OutlinedButton(onClick = { openDownloadsFolder(context) }) {
                        Icon(LierIcons.Folder, contentDescription = null, modifier = Modifier.size(15.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Загрузки", "Downloads"))
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(
                        onClick = { multiFilePicker.launch(arrayOf("*/*")) },
                        colors = ButtonDefaults.buttonColors(containerColor = BgElevated, contentColor = TextMain)
                    ) {
                        Icon(LierIcons.FileDoc, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(tr(lang, "Выбрать файлы", "Pick files"))
                    }
                    Button(
                        onClick = { sendQueue.clear() },
                        colors = ButtonDefaults.buttonColors(containerColor = AccentRed, contentColor = TextMain)
                    ) {
                        Icon(LierIcons.Trash, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("${tr(lang, "Очистить", "Clear")} (${sendQueue.size})")
                    }
                }
                sendQueue.forEach { path ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(LierIcons.FileDoc, contentDescription = null, tint = AccentTeal, modifier = Modifier.size(15.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(File(path).name, style = MaterialTheme.typography.bodySmall, color = TextMain)
                    }
                }
                Button(
                    onClick = { LierSyncService.sendBatchAsync(sendQueue.toList(), targetAddr) },
                    enabled = sendQueue.isNotEmpty() && targetAddr.isNotBlank(),
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                ) {
                    Icon(LierIcons.Send, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("${tr(lang, "Отправить всё", "Send all")} (${sendQueue.size})", fontWeight = FontWeight.SemiBold)
                }
            }
        }

        if (transferHistory.isNotEmpty()) {
            item(key = "qs_hist_header") {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(tr(lang, "История передач", "Transfer history"), fontWeight = FontWeight.SemiBold, color = TextMain)
                    TextButton(onClick = { LierSyncState.clearTransferHistory() }) {
                        Text(tr(lang, "Очистить", "Clear"), color = TextMuted)
                    }
                }
            }
            items(transferHistory.take(15), key = { it.id }) { item ->
                LierCard {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.weight(1f)) {
                            Icon(
                                if (item.isSent) LierIcons.ArrowUp else LierIcons.ArrowDown,
                                contentDescription = null,
                                tint = if (item.isSent) AccentRed else AccentTeal,
                                modifier = Modifier.size(15.dp)
                            )
                            Spacer(Modifier.width(8.dp))
                            Column {
                                Text(item.fileName, fontWeight = FontWeight.SemiBold, color = TextMain)
                                Text(
                                    "${item.time} · ${if (item.isSent) tr(lang, "Отпр.", "Sent") else tr(lang, "Получ.", "Recv")} · ${item.peer}",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = TextMuted
                                )
                            }
                        }
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            if (!item.isSent) {
                                Button(
                                    onClick = { openReceivedFile(context, item.contentUri, item.fileName) },
                                    colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                                ) {
                                    Text(tr(lang, "Открыть", "Open"))
                                }
                            }
                            OutlinedButton(onClick = { openDownloadsFolder(context) }) {
                                Text(tr(lang, "Папка", "Folder"))
                            }
                        }
                    }
                }
            }
        }

        item(key = "qs_logs_header") {
            Text(tr(lang, "Журнал событий", "Event log"), fontWeight = FontWeight.SemiBold, color = TextMain)
        }
        items(logs.take(20)) { line ->
            Text(line, style = MaterialTheme.typography.bodySmall, color = TextMuted)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FolderSyncScreen() {
    val context = LocalContext.current
    val folders by LierSyncState.sharedFolders.collectAsState()
    val targetAddr by LierSyncState.targetAddr.collectAsState()
    val lang by LierSyncState.language.collectAsState()
    var folderIdInput by remember { mutableStateOf("default") }
    var locationMode by remember { mutableStateOf("public") } // "public" | "app" | "custom"
    var pickedTreeUri by remember { mutableStateOf<Uri?>(null) }
    var pickedTreeLabel by remember { mutableStateOf("") }
    var selectedDirCode by remember { mutableStateOf("bidirectional") }
    val scope = rememberCoroutineScope()

    val folderTreePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree()
    ) { uri ->
        if (uri != null) {
            runCatching {
                context.contentResolver.takePersistableUriPermission(
                    uri,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                )
            }
            val docId = runCatching { DocumentsContract.getTreeDocumentId(uri) }.getOrNull().orEmpty()
            pickedTreeUri = uri
            pickedTreeLabel = docId.ifBlank { uri.lastPathSegment ?: tr(lang, "Выбранная папка", "Selected folder") }
            locationMode = "custom"
        }
    }

    val directionOptions = remember(lang) {
        listOf(
            "bidirectional" to tr(lang, "Общая", "Both"),
            "send_only" to tr(lang, "Отправитель", "Send only"),
            "receive_only" to tr(lang, "Получатель", "Receive only")
        )
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item(key = "sync_add") {
            LierCard {
                Text(tr(lang, "Общие папки (блоки 128 КБ)", "Shared Folders (128 KB blocks)"), fontWeight = FontWeight.SemiBold, color = TextMain)
                OutlinedTextField(
                    value = folderIdInput,
                    onValueChange = { folderIdInput = it },
                    label = { Text(tr(lang, "ID папки (одинаковый на ПК и телефоне)", "Folder ID (same on PC & phone)")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )
                Text(tr(lang, "Расположение на телефоне:", "Location on phone:"), style = MaterialTheme.typography.labelMedium, color = TextMuted)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(
                        selected = locationMode == "public",
                        onClick = { locationMode = "public" },
                        label = { Text("Download/LierSync") },
                        colors = FilterChipDefaults.filterChipColors(
                            selectedContainerColor = AccentTeal,
                            selectedLabelColor = BgBase
                        )
                    )
                    FilterChip(
                        selected = locationMode == "app",
                        onClick = { locationMode = "app" },
                        label = { Text(tr(lang, "Папка приложения", "App folder")) },
                        colors = FilterChipDefaults.filterChipColors(
                            selectedContainerColor = AccentTeal,
                            selectedLabelColor = BgBase
                        )
                    )
                    FilterChip(
                        selected = locationMode == "custom",
                        onClick = { folderTreePicker.launch(pickedTreeUri) },
                        label = {
                            Text(
                                if (pickedTreeLabel.isNotBlank()) "${tr(lang, "Своя", "Custom")}: $pickedTreeLabel"
                                else tr(lang, "Выбрать свою папку...", "Choose custom folder...")
                            )
                        },
                        colors = FilterChipDefaults.filterChipColors(
                            selectedContainerColor = AccentTeal,
                            selectedLabelColor = BgBase
                        )
                    )
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && !Environment.isExternalStorageManager()) {
                    OutlinedButton(
                        onClick = {
                            val intent = Intent(
                                Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                                Uri.parse("package:${context.packageName}")
                            )
                            runCatching { context.startActivity(intent) }
                        },
                        border = TealBorder,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text(
                            tr(
                                lang,
                                "Разрешить прямой доступ к любым папкам (опционально)",
                                "Grant direct access to any folder (optional)"
                            ),
                            color = AccentTeal,
                            style = MaterialTheme.typography.labelMedium
                        )
                    }
                }
                Text(tr(lang, "Направление:", "Direction:"), style = MaterialTheme.typography.labelMedium, color = TextMuted)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    directionOptions.forEach { (code, label) ->
                        FilterChip(
                            selected = selectedDirCode == code,
                            onClick = { selectedDirCode = code },
                            label = { Text(label) },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = AccentTeal,
                                selectedLabelColor = BgBase
                            )
                        )
                    }
                }
                Button(
                    onClick = {
                        val mode = locationMode
                        val customUri = pickedTreeUri
                        if (mode == "custom" && customUri == null) {
                            folderTreePicker.launch(null)
                            return@Button
                        }
                        val fid = folderIdInput.trim().ifEmpty { "default" }
                        val dirCode = selectedDirCode
                        val remote = targetAddr
                        scope.launch(Dispatchers.IO) {
                            runCatching {
                                val (resolvedPath, safUriStr) = when (mode) {
                                    "custom" -> LierSyncService.resolvePickedFolder(context, customUri!!, fid)
                                    "public" -> {
                                        @Suppress("DEPRECATION")
                                        val publicDir = File(
                                            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
                                            "LierSync/$fid"
                                        )
                                        val fallbackDir = File(
                                            context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
                                                ?: context.filesDir,
                                            "LierSync_Folders/$fid"
                                        )
                                        val canPublic = (publicDir.exists() || publicDir.mkdirs()) && publicDir.canWrite()
                                        val chosen = if (canPublic) publicDir else fallbackDir.apply { mkdirs() }
                                        chosen.absolutePath to null
                                    }
                                    else -> {
                                        val appDir = File(context.filesDir, "LierSync_Folders/$fid").apply { mkdirs() }
                                        appDir.absolutePath to null
                                    }
                                }

                                LierSyncNative.registerSharedFolder(fid, resolvedPath)
                                val newFolder = MobileSharedFolder(
                                    id = fid,
                                    dirPath = resolvedPath,
                                    remoteAddr = remote,
                                    autoSync = true,
                                    directionCode = dirCode,
                                    status = tr(lang, "Авто", "Auto"),
                                    safTreeUri = safUriStr
                                )
                                LierSyncState.addOrUpdateFolder(
                                    newFolder.copy(status = "${tr(lang, "Авто", "Auto")} · ${newFolder.directionLabel(lang)}")
                                )
                                val label = if (safUriStr != null) "$resolvedPath (SAF)" else resolvedPath
                                LierSyncState.addLog("${tr(lang, "Добавлена папка", "Added folder")} '$fid' ($label)")
                            }.onFailure { err ->
                                LierSyncState.addLog("${tr(lang, "Ошибка добавления папки", "Add folder error")}: ${err.message}")
                            }
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                ) {
                    Icon(LierIcons.Folder, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(tr(lang, "Добавить / Подключить папку", "Add / Connect folder"), fontWeight = FontWeight.SemiBold)
                }
            }
        }

        items(folders, key = { it.id }) { folder ->
            LierCard {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.weight(1f)) {
                        Icon(LierIcons.Folder, contentDescription = null, tint = AccentTeal, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Column {
                            Text("${folder.id} · ${folder.directionLabel(lang)}", fontWeight = FontWeight.Bold, color = TextMain)
                            Text(
                                if (folder.safTreeUri != null) "${folder.dirPath} (SAF)" else folder.dirPath,
                                style = MaterialTheme.typography.labelSmall,
                                color = TextMuted
                            )
                            Text(folder.status, style = MaterialTheme.typography.bodySmall, color = AccentTeal)
                        }
                    }
                    Switch(
                        checked = folder.autoSync,
                        onCheckedChange = { enabled ->
                            LierSyncState.addOrUpdateFolder(
                                folder.copy(
                                    autoSync = enabled,
                                    status = "${if (enabled) tr(lang, "Авто", "Auto") else tr(lang, "Ручной", "Manual")} · ${folder.directionLabel(lang)}"
                                )
                            )
                        },
                        colors = SwitchDefaults.colors(checkedTrackColor = AccentTeal, checkedThumbColor = BgBase)
                    )
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    directionOptions.forEach { (code, label) ->
                        FilterChip(
                            selected = folder.directionCode == code,
                            onClick = {
                                val updated = folder.copy(directionCode = code)
                                LierSyncState.addOrUpdateFolder(
                                    updated.copy(
                                        status = "${if (updated.autoSync) tr(lang, "Авто", "Auto") else tr(lang, "Ручной", "Manual")} · ${updated.directionLabel(lang)}"
                                    )
                                )
                            },
                            label = { Text(label) },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = AccentTeal,
                                selectedLabelColor = BgBase
                            )
                        )
                    }
                }
                folder.preview?.let { p ->
                    Text(
                        "${tr(lang, "Сравнение", "Diff")}: ${tr(lang, "скачать", "download")} ${p.toDownload} | ${tr(lang, "отдать", "upload")} ${p.toUpload} | ${tr(lang, "конфликты", "conflicts")} ${p.conflicts}",
                        style = MaterialTheme.typography.bodySmall,
                        color = TextMuted
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = {
                        scope.launch(Dispatchers.IO) {
                            val raw = LierSyncService.runFolderCompareWithSaf(context, folder)
                            val preview = LierSyncNative.parsePreview(raw)
                            LierSyncState.addOrUpdateFolder(folder.copy(preview = preview))
                        }
                    }) {
                        Icon(Icons.Rounded.Search, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Сравнить", "Compare"))
                    }
                    Button(
                        onClick = {
                            scope.launch(Dispatchers.IO) {
                                val res = LierSyncService.runFolderSyncWithSaf(context, folder)
                                val obj = runCatching { JSONObject(res) }.getOrNull()
                                val status = if (obj?.optBoolean("ok") == true) {
                                    "${tr(lang, "Синхронизировано", "Synced")} (${obj.optInt("chunks")} ${tr(lang, "бл.", "chunks")})"
                                } else {
                                    "${tr(lang, "Ошибка", "Error")}: ${obj?.optString("error") ?: res}"
                                }
                                LierSyncState.addOrUpdateFolder(folder.copy(status = status))
                            }
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                    ) {
                        Icon(LierIcons.Sync, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Синхр.", "Sync"))
                    }
                    IconButton(onClick = { LierSyncState.removeFolder(folder.id) }) {
                        Icon(LierIcons.Trash, contentDescription = tr(lang, "Удалить", "Delete"), tint = AccentRed, modifier = Modifier.size(18.dp))
                    }
                }
            }
        }
    }
}

@Composable
fun ClipboardScreen() {
    val context = LocalContext.current
    val autoClip by LierSyncState.autoClipboard.collectAsState()
    val targetAddr by LierSyncState.targetAddr.collectAsState()
    val history by LierSyncState.clipboardHistory.collectAsState()
    val lang by LierSyncState.language.collectAsState()

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item(key = "cb_controls") {
            LierCard {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(tr(lang, "Автосинхронизация буфера (анти-эхо)", "Auto-sync clipboard (anti-echo)"), color = TextMain)
                    Switch(
                        checked = autoClip,
                        onCheckedChange = { LierSyncState.setAutoClipboard(it) },
                        colors = SwitchDefaults.colors(checkedTrackColor = AccentTeal, checkedThumbColor = BgBase)
                    )
                }
                Text(
                    tr(
                        lang,
                        "Используйте «Отправить буфер» и «Получить буфер» прямо из шторки уведомлений — заходить в приложение не нужно.",
                        "Use \"Send clipboard\" and \"Pull clipboard\" right from the notification shade without opening the app."
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = TextMuted
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Button(
                        onClick = { LierSyncService.pushClipboardNow(context, targetAddr) },
                        modifier = Modifier.weight(1f),
                        colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                    ) {
                        Icon(LierIcons.ArrowUp, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Отправить", "Send"), fontWeight = FontWeight.SemiBold)
                    }
                    OutlinedButton(
                        onClick = { LierSyncService.pullClipboardAsync(context) },
                        modifier = Modifier.weight(1f),
                        border = TealBorder
                    ) {
                        Icon(LierIcons.ArrowDown, contentDescription = null, tint = AccentTeal, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Получить", "Pull"), color = AccentTeal, fontWeight = FontWeight.SemiBold)
                    }
                }
            }
        }
        items(history, key = { "${it.time}_${it.direction}_${it.preview.hashCode()}" }) { rec ->
            LierCard {
                val isSent = rec.direction.contains("Отправ") || rec.direction.contains("Sent")
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        if (isSent) LierIcons.ArrowUp else LierIcons.ArrowDown,
                        contentDescription = null,
                        tint = if (isSent) AccentRed else AccentTeal,
                        modifier = Modifier.size(14.dp)
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        "${if (isSent) tr(lang, "Отправлено", "Sent") else tr(lang, "Получено", "Received")} · ${rec.time} · ${rec.peer}",
                        style = MaterialTheme.typography.labelSmall,
                        color = TextMuted
                    )
                }
                Text(rec.preview, style = MaterialTheme.typography.bodyMedium, color = TextMain)
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ChannelsScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val nodeInfo by LierSyncState.nodeInfo.collectAsState()
    val targetAddr by LierSyncState.targetAddr.collectAsState()
    val policy by LierSyncState.channelPolicy.collectAsState()
    val relay by LierSyncState.relayEndpoint.collectAsState()
    val channels by LierSyncState.channels.collectAsState()
    val btDevices by LierSyncState.btDevices.collectAsState()
    val selectedBtMac by LierSyncState.selectedBtMac.collectAsState()
    val pairingPin by LierSyncState.pairingPin.collectAsState()
    val lang by LierSyncState.language.collectAsState()
    var pingResult by remember { mutableStateOf("") }
    var qrStatus by remember { mutableStateOf("") }
    val myAddr = remember(nodeInfo, lang) { formatMyAddress(nodeInfo, lang) }
    val qrBitmap = remember(myAddr, pairingPin) {
        if (myAddr.contains(":")) {
            val bytes = LierSyncNative.generatePairingQrPng(myAddr, pairingPin, 340)
            if (bytes.isNotEmpty()) BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap() else null
        } else {
            null
        }
    }

    val qrScannerLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            val raw = result.data?.getStringExtra(QrScannerActivity.EXTRA_QR_TEXT).orEmpty()
            if (raw.isNotBlank()) {
                val parsed = runCatching { JSONObject(LierSyncNative.parsePairingUriJson(raw)) }.getOrNull()
                if (parsed?.optBoolean("ok") == true) {
                    val scannedAddr = parsed.optString("addr")
                    val scannedPin = parsed.optString("pin")
                    if (scannedAddr.isNotBlank()) {
                        LierSyncState.setTargetAddr(scannedAddr)
                        qrStatus = "${tr(lang, "Сопряжение с", "Pairing with")} $scannedAddr..."
                        scope.launch(Dispatchers.IO) {
                            val res = LierSyncNative.pairWithPeerJson(scannedAddr, scannedPin)
                            val obj = runCatching { JSONObject(res) }.getOrNull()
                            if (obj?.optBoolean("ok") == true) {
                                val tp = org.larevole.liersync.nativebridge.TrustedPeerItem(
                                    deviceId = obj.optString("device_id"),
                                    deviceName = obj.optString("device_name"),
                                    fingerprint = obj.optString("fingerprint"),
                                    addedAt = obj.optString("added_at")
                                )
                                LierSyncState.addTrustedPeer(tp)
                                qrStatus = "${tr(lang, "Подключено по QR", "Paired via QR")}: ${tp.deviceName}"
                            } else {
                                qrStatus = "${tr(lang, "Адрес сохранён", "Address saved")}: $scannedAddr (${obj?.optString("error") ?: ""})"
                            }
                        }
                    }
                } else {
                    qrStatus = tr(lang, "Неизвестный QR-код", "Invalid QR code")
                }
            }
        }
    }

    val policies = remember(lang) {
        listOf(
            "Авто (Wi-Fi -> USB/Direct -> Bluetooth -> Relay)" to tr(lang, "Авто", "Auto"),
            "Только Wi-Fi / LAN" to "Wi-Fi / LAN",
            "Wi-Fi Direct / USB-модем" to "USB / Direct",
            "Bluetooth (резервный канал)" to "Bluetooth",
            "Удалённый Relay (Интернет)" to "Relay"
        )
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item(key = "ch_diag") {
            LierCard {
                Text(tr(lang, "Подключение и диагностика", "Connection & Diagnostics"), fontWeight = FontWeight.SemiBold, color = TextMain)
                Text("${tr(lang, "Мой адрес", "My address")}: $myAddr", style = MaterialTheme.typography.bodyMedium, color = AccentTeal)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { copyToPhoneClipboard(context, myAddr, "LierSync Address") }) {
                        Icon(LierIcons.Copy, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Копировать", "Copy"))
                    }
                    Button(
                        onClick = {
                            pingResult = "${tr(lang, "Проверка", "Checking")} $targetAddr..."
                            scope.launch(Dispatchers.IO) {
                                val raw = LierSyncNative.pingPeerJson(targetAddr)
                                val obj = runCatching { JSONObject(raw) }.getOrNull()
                                pingResult = if (obj?.optBoolean("ok") == true) {
                                    "OK: ${obj.optString("device_name")} (${obj.optString("os")}) · ${obj.optLong("latency_ms")} ${tr(lang, "мс", "ms")}"
                                } else {
                                    "${tr(lang, "Ошибка", "Error")}: ${obj?.optString("error") ?: raw}"
                                }
                            }
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                    ) {
                        Icon(Icons.Rounded.Search, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr(lang, "Проверить пир", "Ping peer"))
                    }
                }
                if (pingResult.isNotEmpty()) {
                    Text(pingResult, style = MaterialTheme.typography.bodySmall, color = AccentTeal)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(
                        onClick = { qrScannerLauncher.launch(Intent(context, QrScannerActivity::class.java)) },
                        colors = ButtonDefaults.buttonColors(containerColor = AccentRed, contentColor = TextMain)
                    ) {
                        Text(tr(lang, "Сканировать QR", "Scan QR"))
                    }
                    OutlinedButton(onClick = {
                        context.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    }) {
                        Icon(LierIcons.Channels, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Wi-Fi")
                    }
                    OutlinedButton(onClick = {
                        context.startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    }) {
                        Icon(LierIcons.Bluetooth, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Bluetooth")
                    }
                }
                if (qrStatus.isNotEmpty()) {
                    Text(qrStatus, style = MaterialTheme.typography.bodySmall, color = AccentTeal)
                }
                if (qrBitmap != null) {
                    Spacer(Modifier.height(4.dp))
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Image(
                            bitmap = qrBitmap,
                            contentDescription = "LierSync QR",
                            modifier = Modifier
                                .size(116.dp)
                                .clip(RoundedCornerShape(8.dp))
                                .background(Color.White)
                                .padding(4.dp)
                        )
                        Column {
                            Text(tr(lang, "QR этого телефона", "This phone's QR"), fontWeight = FontWeight.SemiBold, color = TextMain)
                            Text("PIN: $pairingPin", color = AccentTeal, fontWeight = FontWeight.Bold)
                            Text(myAddr, style = MaterialTheme.typography.labelSmall, color = TextMuted)
                        }
                    }
                }
            }
        }

        item(key = "ch_policy") {
            LierCard {
                Text(tr(lang, "Маршрутизация каналов", "Channel Routing"), fontWeight = FontWeight.SemiBold, color = TextMain)
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp)
                ) {
                    policies.forEach { (fullName, shortLabel) ->
                        FilterChip(
                            selected = policy == fullName,
                            onClick = { LierSyncState.setChannelPolicy(fullName) },
                            label = { Text(shortLabel) },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = AccentTeal,
                                selectedLabelColor = BgBase
                            )
                        )
                    }
                }
                OutlinedTextField(
                    value = relay,
                    onValueChange = { LierSyncState.setRelayEndpoint(it) },
                    label = { Text(tr(lang, "Relay / Внешний IP", "Relay / External IP")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )
            }
        }

        item(key = "ch_bt") {
            LierCard {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("Bluetooth RFCOMM SPP", fontWeight = FontWeight.SemiBold, color = TextMain)
                    TextButton(onClick = { LierSyncState.setBtDevices(BluetoothSyncManager.listPairedDevices(context)) }) {
                        Text(tr(lang, "Обновить", "Refresh"), color = AccentTeal)
                    }
                }
                if (btDevices.isEmpty()) {
                    Text(
                        tr(
                            lang,
                            "Сопряжённых Bluetooth-устройств не найдено.",
                            "No paired Bluetooth devices found."
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = TextMuted
                    )
                } else {
                    btDevices.forEach { dev ->
                        val selected = dev.mac == selectedBtMac
                        OutlinedButton(
                            onClick = { LierSyncState.selectBtMac(dev.mac) },
                            modifier = Modifier.fillMaxWidth(),
                            border = if (selected) TealBorder else CardBorder
                        ) {
                            Text(
                                "${dev.name} — ${dev.mac}",
                                color = if (selected) AccentTeal else TextMain
                            )
                        }
                    }
                    Button(
                        onClick = { LierSyncService.sendClipboardViaBluetooth(context, selectedBtMac) },
                        modifier = Modifier.fillMaxWidth(),
                        colors = ButtonDefaults.buttonColors(containerColor = AccentTeal, contentColor = BgBase)
                    ) {
                        Icon(LierIcons.Bluetooth, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(6.dp))
                        Text(tr(lang, "Отправить буфер по Bluetooth", "Send clipboard via Bluetooth"))
                    }
                }
            }
        }

        items(channels, key = { it.medium }) { ch ->
            LierCard {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            channelIcon(ch.medium),
                            contentDescription = null,
                            tint = if (ch.available) AccentTeal else TextMuted,
                            modifier = Modifier.size(18.dp)
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(ch.medium, fontWeight = FontWeight.Bold, color = TextMain)
                    }
                    Text(
                        if (ch.available) tr(lang, "Активен", "Active") else tr(lang, "Ожидание", "Standby"),
                        style = MaterialTheme.typography.labelSmall,
                        color = if (ch.available) AccentTeal else TextMuted
                    )
                }
                Text(ch.endpointHint, style = MaterialTheme.typography.bodySmall, color = TextMuted)
                Text(ch.bandwidthNote, style = MaterialTheme.typography.bodySmall, color = TextMuted)
            }
        }
    }
}

private fun channelIcon(medium: String): ImageVector = when {
    medium.contains("Bluetooth", ignoreCase = true) -> LierIcons.Bluetooth
    medium.contains("Relay", ignoreCase = true) -> LierIcons.Globe
    else -> LierIcons.Channels
}

private fun openDownloadsFolder(context: Context) {
    val opened = runCatching {
        context.startActivity(
            Intent(DownloadManager.ACTION_VIEW_DOWNLOADS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
        true
    }.getOrDefault(false)
    if (!opened) {
        LierSyncState.addLog(
            tr(
                LierSyncState.language.value,
                "Откройте папку Download/LierSync в приложении «Файлы»",
                "Open Download/LierSync in your Files app"
            )
        )
    }
}

private fun openReceivedFile(context: Context, uriStr: String?, fileName: String) {
    if (uriStr.isNullOrBlank()) {
        openDownloadsFolder(context)
        return
    }
    val uri = Uri.parse(uriStr)
    val mime = context.contentResolver.getType(uri) ?: run {
        val ext = fileName.substringAfterLast('.', "").lowercase()
        android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
    }
    val intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, mime)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    runCatching {
        val title = "${tr(LierSyncState.language.value, "Открыть", "Open")} $fileName"
        context.startActivity(Intent.createChooser(intent, title).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }.onFailure {
        openDownloadsFolder(context)
    }
}

private fun copyUriToStaging(context: Context, uri: Uri): String? = runCatching {
    val name = context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
        val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
    } ?: "file_${System.currentTimeMillis()}"

    val staging = File(context.cacheDir, "send_staging").apply { mkdirs() }
    val dest = File(staging, name)
    context.contentResolver.openInputStream(uri)?.use { input ->
        dest.outputStream().use { output -> input.copyTo(output) }
    }
    dest.absolutePath
}.getOrNull()
