// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

object LierIcons {
    private val stroke = SolidColor(Color.White)

    private fun icon(block: ImageVector.Builder.() -> Unit): ImageVector =
        ImageVector.Builder(
            defaultWidth = 24.dp,
            defaultHeight = 24.dp,
            viewportWidth = 24f,
            viewportHeight = 24f
        ).apply(block).build()

    val Send: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(22f, 2f)
                lineTo(11f, 13f)
                moveTo(22f, 2f)
                lineTo(15f, 22f)
                lineTo(11f, 13f)
                lineTo(2f, 9f)
                close()
            }
        }
    }

    val Sync: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(21f, 12f)
                arcTo(9f, 9f, 0f, isMoreThanHalf = false, isPositiveArc = false, 6.26f, 5.26f)
                lineTo(3f, 8f)
                moveTo(3f, 3f)
                verticalLineTo(8f)
                horizontalLineTo(8f)
                moveTo(3f, 12f)
                arcTo(9f, 9f, 0f, isMoreThanHalf = false, isPositiveArc = false, 17.74f, 18.74f)
                lineTo(21f, 16f)
                moveTo(16f, 16f)
                horizontalLineTo(21f)
                verticalLineTo(21f)
            }
        }
    }

    val Clipboard: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(9f, 2f)
                horizontalLineTo(15f)
                verticalLineTo(5f)
                horizontalLineTo(9f)
                close()
                moveTo(16f, 4f)
                horizontalLineTo(18f)
                arcTo(2f, 2f, 0f, false, true, 20f, 6f)
                verticalLineTo(20f)
                arcTo(2f, 2f, 0f, false, true, 18f, 22f)
                horizontalLineTo(6f)
                arcTo(2f, 2f, 0f, false, true, 4f, 20f)
                verticalLineTo(6f)
                arcTo(2f, 2f, 0f, false, true, 6f, 4f)
                horizontalLineTo(8f)
            }
        }
    }

    val Channels: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(5f, 12.55f)
                arcTo(11f, 11f, 0f, false, true, 19.08f, 12.55f)
                moveTo(1.42f, 9f)
                arcTo(16f, 16f, 0f, false, true, 22.58f, 9f)
                moveTo(8.53f, 16.11f)
                arcTo(6f, 6f, 0f, false, true, 15.48f, 16.11f)
                moveTo(12f, 20f)
                horizontalLineTo(12.01f)
            }
        }
    }

    val FileDoc: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(14f, 2f)
                horizontalLineTo(6f)
                arcTo(2f, 2f, 0f, false, false, 4f, 4f)
                verticalLineTo(20f)
                arcTo(2f, 2f, 0f, false, false, 6f, 22f)
                horizontalLineTo(18f)
                arcTo(2f, 2f, 0f, false, false, 20f, 20f)
                verticalLineTo(8f)
                close()
                moveTo(14f, 2f)
                verticalLineTo(8f)
                horizontalLineTo(20f)
            }
        }
    }

    val Folder: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(20f, 20f)
                arcTo(2f, 2f, 0f, false, false, 22f, 18f)
                verticalLineTo(8f)
                arcTo(2f, 2f, 0f, false, false, 20f, 6f)
                horizontalLineTo(12f)
                lineTo(10f, 3.5f)
                horizontalLineTo(4f)
                arcTo(2f, 2f, 0f, false, false, 2f, 5.5f)
                verticalLineTo(18f)
                arcTo(2f, 2f, 0f, false, false, 4f, 20f)
                close()
            }
        }
    }

    val Copy: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(8f, 8f)
                horizontalLineTo(20f)
                verticalLineTo(20f)
                horizontalLineTo(8f)
                close()
                moveTo(4f, 16f)
                horizontalLineTo(3f)
                arcTo(1f, 1f, 0f, false, true, 2f, 15f)
                verticalLineTo(4f)
                arcTo(2f, 2f, 0f, false, true, 4f, 2f)
                horizontalLineTo(15f)
                arcTo(1f, 1f, 0f, false, true, 16f, 3f)
                verticalLineTo(4f)
            }
        }
    }

    val Trash: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(3f, 6f)
                horizontalLineTo(21f)
                moveTo(8f, 6f)
                verticalLineTo(4f)
                horizontalLineTo(16f)
                verticalLineTo(6f)
                moveTo(6f, 6f)
                verticalLineTo(20f)
                horizontalLineTo(18f)
                verticalLineTo(6f)
            }
        }
    }

    val ArrowUp: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(12f, 19f)
                verticalLineTo(5f)
                moveTo(5f, 12f)
                lineTo(12f, 5f)
                lineTo(19f, 12f)
            }
        }
    }

    val ArrowDown: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(12f, 5f)
                verticalLineTo(19f)
                moveTo(19f, 12f)
                lineTo(12f, 19f)
                lineTo(5f, 12f)
            }
        }
    }

    val Bluetooth: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(7f, 7f)
                lineTo(17f, 17f)
                lineTo(12f, 22f)
                verticalLineTo(2f)
                lineTo(17f, 7f)
                lineTo(7f, 17f)
            }
        }
    }

    val Globe: ImageVector by lazy {
        icon {
            path(stroke = stroke, strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
                moveTo(12f, 2f)
                arcTo(10f, 10f, 0f, true, false, 12f, 22f)
                arcTo(10f, 10f, 0f, true, false, 12f, 2f)
                moveTo(2f, 12f)
                horizontalLineTo(22f)
            }
        }
    }
}
