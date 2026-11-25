// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.ui

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import org.larevole.liersync.service.LierSyncService
import org.larevole.liersync.service.LierSyncState
import org.larevole.liersync.service.tr

/**
 * Isolated translucent Activity in its own taskAffinity (`org.larevole.liersync.clip`).
 * Grants brief OS window focus to legally read ClipboardManager on Android 10+
 * without bringing MainActivity to the foreground, then closes immediately.
 */
class ClipboardSyncActivity : ComponentActivity() {
    private var handled = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val sharedText = intent?.getStringExtra(Intent.EXTRA_TEXT)
        if (!sharedText.isNullOrBlank()) {
            handled = true
            LierSyncService.pushTextManual(applicationContext, sharedText)
            finishQuietly()
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && !handled) {
            handled = true
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            val img = LierSyncService.readClipboardImagePngFromContext(this, cm)
            if (img != null) {
                LierSyncService.pushImageManual(applicationContext, img.first, img.second, img.third)
            } else {
                val text = cm?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)
                    ?.coerceToText(this)?.toString()
                if (!text.isNullOrEmpty()) {
                    LierSyncService.pushTextManual(applicationContext, text)
                } else {
                    LierSyncService.showToastGlobal(
                        applicationContext,
                        tr(LierSyncState.language.value, "Буфер обмена пуст", "Clipboard is empty")
                    )
                }
            }
            finishQuietly()
        }
    }

    @Suppress("DEPRECATION")
    private fun finishQuietly() {
        finishAndRemoveTask()
        overridePendingTransition(0, 0)
    }
}
