// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import org.larevole.liersync.service.LierSyncService
import org.larevole.liersync.service.LierSyncState
import org.larevole.liersync.service.tr
import java.io.File

/**
 * Handles Android system "Share -> LierSync" (`ACTION_SEND` / `ACTION_SEND_MULTIPLE`)
 * for text, images, and arbitrary files.
 */
class ShareReceiverActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val svcIntent = Intent(this, LierSyncService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(svcIntent)
        } else {
            startService(svcIntent)
        }

        val l = LierSyncState.language.value
        val act = intent?.action

        val uris = buildList {
            if (act == Intent.ACTION_SEND) {
                @Suppress("DEPRECATION")
                val u = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    intent.getParcelableExtra(Intent.EXTRA_STREAM)
                }
                if (u != null) add(u)
            } else if (act == Intent.ACTION_SEND_MULTIPLE) {
                @Suppress("DEPRECATION")
                val list = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)
                }
                if (list != null) addAll(list)
            }
        }

        if (uris.isNotEmpty()) {
            val staged = uris.mapNotNull { copySharedUri(it) }
            if (staged.isNotEmpty()) {
                val target = LierSyncService.activeTargetAddrs().firstOrNull()
                if (!target.isNullOrBlank()) {
                    LierSyncService.sendBatchAsync(staged, target)
                    LierSyncService.showToastGlobal(
                        applicationContext,
                        "${tr(l, "Отправка файлов", "Sending files")} (${staged.size}) -> $target"
                    )
                } else {
                    LierSyncState.addPendingShareFiles(staged)
                    LierSyncService.showToastGlobal(
                        applicationContext,
                        tr(l, "Выберите получателя в LierSync", "Select a recipient in LierSync")
                    )
                    startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                }
            }
        } else {
            val sharedText = intent?.getStringExtra(Intent.EXTRA_TEXT)
            if (!sharedText.isNullOrBlank()) {
                LierSyncService.pushTextManual(applicationContext, sharedText)
            }
        }

        @Suppress("DEPRECATION")
        finishAndRemoveTask()
        @Suppress("DEPRECATION")
        overridePendingTransition(0, 0)
    }

    private fun copySharedUri(uri: Uri): String? = runCatching {
        val name = contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (cursor.moveToFirst() && idx >= 0) cursor.getString(idx) else null
        } ?: "shared_${System.currentTimeMillis()}"
        val staging = File(cacheDir, "send_staging").apply { mkdirs() }
        val dest = File(staging, name)
        contentResolver.openInputStream(uri)?.use { inp ->
            dest.outputStream().use { out -> inp.copyTo(out) }
        }
        dest.absolutePath
    }.getOrNull()
}
