// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only
package org.larevole.liersync.ui

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.ImageFormat
import android.graphics.SurfaceTexture
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.media.ImageReader
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.view.Gravity
import android.view.Surface
import android.view.TextureView
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import org.larevole.liersync.service.LierSyncState
import org.larevole.liersync.service.tr
import java.util.concurrent.atomic.AtomicBoolean

class QrScannerActivity : ComponentActivity() {
    companion object {
        const val EXTRA_QR_TEXT = "qr_text"
    }

    private lateinit var textureView: TextureView
    private var cameraDevice: CameraDevice? = null
    private var captureSession: CameraCaptureSession? = null
    private var imageReader: ImageReader? = null
    private var bgThread: HandlerThread? = null
    private var bgHandler: Handler? = null
    private val found = AtomicBoolean(false)

    private val reader = MultiFormatReader().apply {
        setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
    }

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (granted) {
            tryOpenCamera()
        } else {
            finish()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val l = LierSyncState.language.value

        textureView = TextureView(this)
        val root = FrameLayout(this).apply {
            setBackgroundColor(0xFF121316.toInt())
            addView(
                textureView,
                FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.MATCH_PARENT
                )
            )
            addView(
                TextView(this@QrScannerActivity).apply {
                    text = tr(l, "Наведите камеру на QR-код LierSync", "Point camera at LierSync QR code")
                    setTextColor(0xFF39C5BB.toInt())
                    setBackgroundColor(0xAA121316.toInt())
                    setPadding(32, 24, 32, 24)
                    gravity = Gravity.CENTER
                },
                FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT,
                    Gravity.BOTTOM
                )
            )
        }
        setContentView(root)

        textureView.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(st: SurfaceTexture, w: Int, h: Int) {
                checkPermAndOpen()
            }

            override fun onSurfaceTextureSizeChanged(st: SurfaceTexture, w: Int, h: Int) {}
            override fun onSurfaceTextureDestroyed(st: SurfaceTexture): Boolean = true
            override fun onSurfaceTextureUpdated(st: SurfaceTexture) {}
        }
    }

    private fun checkPermAndOpen() {
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            tryOpenCamera()
        } else {
            permissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    @SuppressLint("MissingPermission")
    private fun tryOpenCamera() {
        if (!textureView.isAvailable || cameraDevice != null) return
        bgThread = HandlerThread("LierQrScan").also { it.start() }
        bgHandler = Handler(bgThread!!.looper)

        val mgr = getSystemService(Context.CAMERA_SERVICE) as CameraManager
        val camId = mgr.cameraIdList.firstOrNull { id ->
            mgr.getCameraCharacteristics(id).get(CameraCharacteristics.LENS_FACING) ==
                CameraCharacteristics.LENS_FACING_BACK
        } ?: mgr.cameraIdList.firstOrNull() ?: run {
            finish()
            return
        }

        val w = 1280
        val h = 720
        imageReader = ImageReader.newInstance(w, h, ImageFormat.YUV_420_888, 2).apply {
            setOnImageAvailableListener({ ir ->
                val img = ir.acquireLatestImage() ?: return@setOnImageAvailableListener
                try {
                    if (found.get()) return@setOnImageAvailableListener
                    val yPlane = img.planes[0]
                    val yBuf = yPlane.buffer
                    val rowStride = yPlane.rowStride
                    val data = ByteArray(yBuf.remaining())
                    yBuf.get(data)
                    val src = PlanarYUVLuminanceSource(
                        data, rowStride, img.height,
                        0, 0, img.width, img.height, false
                    )
                    val bmp = BinaryBitmap(HybridBinarizer(src))
                    val res = runCatching { reader.decodeWithState(bmp) }.getOrNull()
                    if (res != null && found.compareAndSet(false, true)) {
                        val out = Intent().putExtra(EXTRA_QR_TEXT, res.text)
                        setResult(Activity.RESULT_OK, out)
                        finish()
                    }
                } finally {
                    img.close()
                }
            }, bgHandler)
        }

        mgr.openCamera(camId, object : CameraDevice.StateCallback() {
            override fun onOpened(cam: CameraDevice) {
                cameraDevice = cam
                val st = textureView.surfaceTexture ?: return
                st.setDefaultBufferSize(w, h)
                val previewSurface = Surface(st)
                val analyzeSurface = imageReader!!.surface
                val req = cam.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW).apply {
                    addTarget(previewSurface)
                    addTarget(analyzeSurface)
                    set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE)
                }
                @Suppress("DEPRECATION")
                cam.createCaptureSession(
                    listOf(previewSurface, analyzeSurface),
                    object : CameraCaptureSession.StateCallback() {
                        override fun onConfigured(session: CameraCaptureSession) {
                            captureSession = session
                            session.setRepeatingRequest(req.build(), null, bgHandler)
                        }

                        override fun onConfigureFailed(session: CameraCaptureSession) {}
                    },
                    bgHandler
                )
            }

            override fun onDisconnected(cam: CameraDevice) {
                cam.close()
                cameraDevice = null
            }

            override fun onError(cam: CameraDevice, error: Int) {
                cam.close()
                cameraDevice = null
            }
        }, bgHandler)
    }

    override fun onDestroy() {
        runCatching { captureSession?.close() }
        runCatching { cameraDevice?.close() }
        runCatching { imageReader?.close() }
        bgThread?.quitSafely()
        super.onDestroy()
    }
}
