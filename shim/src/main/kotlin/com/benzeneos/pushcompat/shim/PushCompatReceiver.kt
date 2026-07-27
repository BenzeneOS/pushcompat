package com.benzeneos.pushcompat.shim

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * BroadcastReceiver for UnifiedPush messages.
 *
 * Registered in AndroidManifest.xml by the patcher:
 * <receiver android:name="com.benzeneos.pushcompat.shim.PushCompatReceiver" android:exported="true">
 *     <intent-filter>
 *         <action android:name="org.unifiedpush.android.connector.MESSAGE"/>
 *         <action android:name="org.unifiedpush.android.connector.NEW_ENDPOINT"/>
 *         <action android:name="org.unifiedpush.android.connector.REGISTRATION_FAILED"/>
 *         <action android:name="org.unifiedpush.android.connector.UNREGISTERED"/>
 *     </intent-filter>
 * </receiver>
 */
class PushCompatReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "PushCompat"

        // UnifiedPush actions
        private const val ACTION_MESSAGE = "org.unifiedpush.android.connector.MESSAGE"
        private const val ACTION_NEW_ENDPOINT = "org.unifiedpush.android.connector.NEW_ENDPOINT"
        private const val ACTION_REGISTRATION_FAILED = "org.unifiedpush.android.connector.REGISTRATION_FAILED"
        private const val ACTION_UNREGISTERED = "org.unifiedpush.android.connector.UNREGISTERED"

        // ntfy uses "bytesMessage" for raw bytes
        private const val EXTRA_BYTES_MESSAGE = "bytesMessage"
        private const val EXTRA_MESSAGE = "message"
        private const val EXTRA_ENDPOINT = "endpoint"
    }

    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return

        Log.d(TAG, "Received action: $action")

        when (action) {
            ACTION_MESSAGE -> handleMessage(context, intent)
            ACTION_NEW_ENDPOINT -> handleNewEndpoint(context, intent)
            ACTION_REGISTRATION_FAILED -> handleRegistrationFailed(context, intent)
            ACTION_UNREGISTERED -> handleUnregistered(context)
        }
    }

    private fun handleMessage(context: Context, intent: Intent) {
        // Try bytes first (ntfy uses "bytesMessage")
        val bytes = intent.getByteArrayExtra(EXTRA_BYTES_MESSAGE)
        if (bytes != null) {
            PushCompatShim.onMessage(context, bytes)
            return
        }

        // Fallback to string message
        val message = intent.getStringExtra(EXTRA_MESSAGE)
        if (message != null) {
            PushCompatShim.onMessage(context, message.toByteArray())
            return
        }

        Log.w(TAG, "MESSAGE intent without message data")
    }

    private fun handleNewEndpoint(context: Context, intent: Intent) {
        val endpoint = intent.getStringExtra(EXTRA_ENDPOINT)
        if (endpoint != null) {
            PushCompatShim.onNewEndpoint(context, endpoint)
        } else {
            Log.w(TAG, "NEW_ENDPOINT intent without endpoint")
        }
    }

    private fun handleRegistrationFailed(context: Context, intent: Intent) {
        val reason = intent.getStringExtra(EXTRA_MESSAGE)
        PushCompatShim.onRegistrationFailed(context, reason)
    }

    private fun handleUnregistered(context: Context) {
        PushCompatShim.onUnregistered(context)
    }
}
