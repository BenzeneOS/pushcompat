package com.benzeneos.pushcompat.shim

import android.app.BroadcastOptions
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.os.Build
import android.util.Log
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.security.SecureRandom
import java.util.UUID
import java.util.concurrent.Executors
import org.json.JSONObject

/**
 * PushCompat shim
 *
 * Receives push over UnifiedPush and hands it to the host app's own Firebase SDK by
 * broadcasting the SDK's dispatch actions in-process. Injected by the PushCompat patcher.
 */
object PushCompatShim : android.app.Application.ActivityLifecycleCallbacks {
    private const val TAG = "PushCompat"
    private const val PREFS_NAME = "pushcompat_prefs"

    private const val KEY_ENDPOINT = "up_endpoint"
    private const val KEY_TOKEN = "up_token"
    private const val KEY_FCM_TOKEN = "fcm_token"
    private const val KEY_BRIDGE_FCM_TOKEN = "bridge_fcm_token"
    private const val KEY_BRIDGE_URL = "bridge_url"
    private const val KEY_DISTRIBUTOR = "distributor"
    private const val KEY_FIREBASE_APP_ID = "firebase_app_id"
    private const val KEY_FIREBASE_PROJECT_ID = "firebase_project_id"
    private const val KEY_FIREBASE_API_KEY = "firebase_api_key"
    private const val KEY_C2DM_RECEIVER = "c2dm_receiver"
    private const val KEY_CERT_SHA1 = "cert_sha1"
    private const val KEY_INSTALL_ID = "install_id"
    private const val KEY_INSTALL_SECRET = "install_secret"
    private const val KEY_LAST_REGISTERED_MS = "last_registered_ms"
    private const val KEY_LAST_REGISTRATION_SIGNATURE = "last_registration_signature"

    // The Firebase SDK's own dispatch actions. Delivering through these lets the
    // unmodified SDK build the RemoteMessage and call onNewToken/onMessageReceived,
    // so nothing here needs to know anything about the host app.
    private const val ACTION_NEW_TOKEN = "com.google.firebase.messaging.NEW_TOKEN"
    private const val ACTION_C2DM_RECEIVE = "com.google.android.c2dm.intent.RECEIVE"
    private const val EXTRA_TOKEN = "token"

    // Actions we SEND to the distributor (ntfy)
    private const val ACTION_REGISTER = "org.unifiedpush.android.distributor.REGISTER"
    private const val ACTION_UNREGISTER = "org.unifiedpush.android.distributor.UNREGISTER"

    private const val DEFAULT_DISTRIBUTOR = "io.heckel.ntfy"

    private val executor = Executors.newSingleThreadExecutor()

    // Held so the patched FirebaseMessaging.getToken() can answer without a Context.
    @Volatile
    private var appContext: Context? = null

    /**
     * Answer for the patched FirebaseMessaging.getToken().
     *
     * Apps chain real work onto that Task — GitHub creates all of its notification
     * channels in the completion callback — and without GMS it never completes, so the
     * channels are never registered and every notification is dropped with
     * "No Channel found". Returning the bridge token makes that callback run.
     */
    @JvmStatic
    fun currentToken(): String {
        val context = appContext ?: return ""
        return getPrefs(context).getString(KEY_BRIDGE_FCM_TOKEN, null) ?: ""
    }

    // The app drops a token delivered while the user is signed out, and nothing else
    // will ever trigger another delivery — getToken() cannot succeed without GMS.
    // Re-announce on activity resume so a sign-in that happens mid-session is covered.
    private const val REDELIVER_INTERVAL_MS = 60000L
    private const val HEARTBEAT_INTERVAL_MS = 24L * 60L * 60L * 1000L

    @Volatile
    private var lastDeliveryMs = 0L

    private fun bridgeHeartbeatDue(prefs: SharedPreferences): Boolean {
        val lastRegistered = prefs.getLong(KEY_LAST_REGISTERED_MS, 0L)
        return System.currentTimeMillis() - lastRegistered > HEARTBEAT_INTERVAL_MS
    }

    override fun onActivityResumed(activity: android.app.Activity) {
        val prefs = getPrefs(activity)
        if (bridgeHeartbeatDue(prefs)
            && notEmpty(prefs.getString(KEY_ENDPOINT, null))
        ) {
            sendRegistrationToBridge(activity)
        }

        val token = prefs.getString(KEY_BRIDGE_FCM_TOKEN, null)
        if (!notEmpty(token)) return
        val now = System.currentTimeMillis()
        if (now - lastDeliveryMs < REDELIVER_INTERVAL_MS) return
        lastDeliveryMs = now
        deliverToken(activity, token!!)
    }

    override fun onActivityCreated(activity: android.app.Activity, state: android.os.Bundle?) {}
    override fun onActivityStarted(activity: android.app.Activity) {}
    override fun onActivityPaused(activity: android.app.Activity) {}
    override fun onActivityStopped(activity: android.app.Activity) {}
    override fun onActivitySaveInstanceState(activity: android.app.Activity, state: android.os.Bundle) {}
    override fun onActivityDestroyed(activity: android.app.Activity) {}

    // Helper to avoid kotlin stdlib StringsKt
    private fun notEmpty(s: String?): Boolean = s != null && s.length > 0
    private fun preview(s: String?): String {
        if (s == null) return "null"
        return if (s.length > 20) s.substring(0, 20) + "..." else s
    }

    private const val HEX_DIGITS = "0123456789abcdef"

    private fun toHex(bytes: ByteArray): String {
        val sb = StringBuilder(bytes.size * 2)
        var i = 0
        while (i < bytes.size) {
            val b = bytes[i].toInt() and 0xff
            sb.append(HEX_DIGITS[b ushr 4])
            sb.append(HEX_DIGITS[b and 0x0f])
            i++
        }
        return sb.toString()
    }

    private fun randomHex(byteCount: Int): String {
        val bytes = ByteArray(byteCount)
        SecureRandom().nextBytes(bytes)
        return toHex(bytes)
    }

    /**
     * Opaque per-install identity. Random rather than derived from anything on the
     * device, so the bridge cannot link two installs to each other or to hardware.
     * Raw hex, not a UUID: the bridge rejects the dashes.
     */
    private fun installId(prefs: SharedPreferences): String {
        val existing = prefs.getString(KEY_INSTALL_ID, null)
        if (notEmpty(existing)) return existing!!
        val fresh = randomHex(16)
        prefs.edit().putString(KEY_INSTALL_ID, fresh).apply()
        return fresh
    }

    private fun installSecret(prefs: SharedPreferences): String {
        val existing = prefs.getString(KEY_INSTALL_SECRET, null)
        if (notEmpty(existing)) return existing!!
        val fresh = randomHex(32)
        prefs.edit().putString(KEY_INSTALL_SECRET, fresh).apply()
        return fresh
    }

    /**
     * Configure the shim with Firebase credentials, FCM service class, and original cert SHA1.
     */
    @JvmStatic
    fun configure(
        context: Context,
        bridgeUrl: String,
        distributor: String,
        firebaseAppId: String?,
        firebaseProjectId: String?,
        firebaseApiKey: String?,
        c2dmReceiver: String?,
        certSha1: String?
    ) {
        val prefs = getPrefs(context)
        val editor = prefs.edit()
        editor.putString(KEY_BRIDGE_URL, bridgeUrl)
        editor.putString(KEY_DISTRIBUTOR, if (distributor.length > 0) distributor else DEFAULT_DISTRIBUTOR)
        if (notEmpty(firebaseAppId)) editor.putString(KEY_FIREBASE_APP_ID, firebaseAppId)
        if (notEmpty(firebaseProjectId)) editor.putString(KEY_FIREBASE_PROJECT_ID, firebaseProjectId)
        if (notEmpty(firebaseApiKey)) editor.putString(KEY_FIREBASE_API_KEY, firebaseApiKey)
        if (notEmpty(c2dmReceiver)) editor.putString(KEY_C2DM_RECEIVER, c2dmReceiver)
        if (notEmpty(certSha1)) editor.putString(KEY_CERT_SHA1, certSha1)
        editor.apply()

        val app = context.applicationContext
        appContext = app
        if (app is android.app.Application) {
            app.registerActivityLifecycleCallbacks(this)
        }

        Log.i(TAG, "Configured: bridge=$bridgeUrl, distributor=$distributor, firebase_app_id=${preview(firebaseAppId)}, c2dm_receiver=$c2dmReceiver, cert=${preview(certSha1)}")
    }



    /**
     * Get the FCM token that the app should use (bridge's token).
     */
    @JvmStatic
    fun getEffectiveFcmToken(context: Context): String? {
        val prefs = getPrefs(context)
        val bridgeToken = prefs.getString(KEY_BRIDGE_FCM_TOKEN, null)
        return if (bridgeToken != null) bridgeToken else prefs.getString(KEY_FCM_TOKEN, null)
    }


    /**
     * Register with UnifiedPush distributor.
     */
    @JvmStatic
    fun register(context: Context) {
        Log.i(TAG, "Registering with UnifiedPush")

        val prefs = getPrefs(context)
        var token = prefs.getString(KEY_TOKEN, null)
        if (token == null) {
            token = UUID.randomUUID().toString()
            prefs.edit().putString(KEY_TOKEN, token).apply()
        }

        val distributor = prefs.getString(KEY_DISTRIBUTOR, DEFAULT_DISTRIBUTOR)!!

        val intent = Intent(ACTION_REGISTER)
        intent.`package` = distributor
        intent.putExtra("token", token)
        intent.putExtra("application", context.packageName)

        if (Build.VERSION.SDK_INT >= 34) {
            val options = BroadcastOptions.makeBasic()
            options.setShareIdentityEnabled(true)
            context.sendBroadcast(intent, null, options.toBundle())
        } else {
            context.sendBroadcast(intent)
        }

        Log.d(TAG, "Sent REGISTER to $distributor with token $token")
    }

    /**
     * Unregister from UnifiedPush distributor.
     */
    @JvmStatic
    fun unregister(context: Context) {
        val prefs = getPrefs(context)
        val token = prefs.getString(KEY_TOKEN, null)
        if (token == null) return

        val distributor = prefs.getString(KEY_DISTRIBUTOR, DEFAULT_DISTRIBUTOR)!!

        val intent = Intent(ACTION_UNREGISTER)
        intent.`package` = distributor
        intent.putExtra("token", token)
        intent.putExtra("application", context.packageName)

        context.sendBroadcast(intent)
        Log.i(TAG, "Sent UNREGISTER to $distributor")
    }

    /**
     * Called when we receive a new endpoint from UnifiedPush.
     */
    @JvmStatic
    fun onNewEndpoint(context: Context, endpoint: String) {
        Log.i(TAG, "New endpoint: $endpoint")
        getPrefs(context).edit().putString(KEY_ENDPOINT, endpoint).apply()
        sendRegistrationToBridge(context)
    }

    /**
     * Called when we receive a message from UnifiedPush.
     */
    @JvmStatic
    fun onMessage(context: Context, message: ByteArray) {
        Log.d(TAG, "UP message received: ${message.size} bytes")

        // Log message content for debugging
        val messageStr = String(message)
        Log.d(TAG, "Message content: $messageStr")

        val receiver = getPrefs(context).getString(KEY_C2DM_RECEIVER, null)
        if (!notEmpty(receiver)) {
            Log.w(TAG, "No c2dm receiver configured")
            showNotificationFromMessage(context, messageStr)
            return
        }

        try {
            val intent = Intent(ACTION_C2DM_RECEIVE)
            intent.setClassName(context.packageName, receiver!!)
            putPayloadExtras(intent, messageStr)
            context.sendBroadcast(intent)
            Log.i(TAG, "Broadcast RECEIVE to $receiver")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to broadcast RECEIVE", e)
            showNotificationFromMessage(context, messageStr)
        }
    }

    /**
     * RemoteMessage is reconstructed by the SDK from flat string extras, so the payload's
     * top-level JSON object is unpacked one key per extra rather than passed as a blob.
     */
    private fun putPayloadExtras(intent: Intent, messageStr: String) {
        try {
            val json = JSONObject(messageStr)
            val keys = json.keys()
            while (keys.hasNext()) {
                val key = keys.next()
                intent.putExtra(key, json.optString(key))
            }
        } catch (e: Exception) {
            Log.w(TAG, "Payload is not a JSON object, forwarding as single extra")
            intent.putExtra("message", messageStr)
        }
    }

    /**
     * Parse FCM message data and show a notification directly.
     * Fallback when we can't start the app's FCM service.
     */
    private fun showNotificationFromMessage(context: Context, messageStr: String) {
        try {
            val json = JSONObject(messageStr)

            // Try to extract notification fields from FCM data
            val title = json.optString("title", json.optString("notification_title", "GitHub"))
            val body = json.optString("body", json.optString("notification_body", json.optString("message", "")))

            if (body.length > 0) {
                showNotification(context, title, body)
            } else {
                Log.d(TAG, "No notification body in message, raw data only")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to parse message as JSON", e)
        }
    }

    /**
     * Show a notification using Android's notification API.
     */
    private fun showNotification(context: Context, title: String, body: String) {
        try {
            val notificationManager = context.getSystemService(Context.NOTIFICATION_SERVICE) as android.app.NotificationManager

            // Create notification channel for Android O+
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                val channel = android.app.NotificationChannel(
                    "pushcompat_channel",
                    "Push Notifications",
                    android.app.NotificationManager.IMPORTANCE_DEFAULT
                )
                notificationManager.createNotificationChannel(channel)
            }

            // Create launch intent
            val launchIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            val pendingIntent = if (launchIntent != null) {
                android.app.PendingIntent.getActivity(
                    context, 0, launchIntent,
                    android.app.PendingIntent.FLAG_UPDATE_CURRENT or android.app.PendingIntent.FLAG_IMMUTABLE
                )
            } else null

            val notification = android.app.Notification.Builder(context, "pushcompat_channel")
                .setContentTitle(title)
                .setContentText(body)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setAutoCancel(true)
                .setContentIntent(pendingIntent)
                .build()

            notificationManager.notify(System.currentTimeMillis().toInt(), notification)
            Log.i(TAG, "Showed notification: $title - $body")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to show notification", e)
        }
    }

    /**
     * Called when registration fails.
     */
    @JvmStatic
    fun onRegistrationFailed(context: Context, reason: String?) {
        Log.e(TAG, "Registration failed: $reason")
    }

    /**
     * Called when we're unregistered.
     */
    @JvmStatic
    fun onUnregistered(context: Context) {
        Log.i(TAG, "Unregistered from UnifiedPush")
        sendUnregisterToBridge(context)
        // KEY_INSTALL_ID and KEY_INSTALL_SECRET deliberately survive: re-registering
        // must reclaim the same bridge row, not strand it and start a new one.
        val editor = getPrefs(context).edit()
        editor.remove(KEY_ENDPOINT)
        editor.remove(KEY_TOKEN)
        editor.remove(KEY_BRIDGE_FCM_TOKEN)
        editor.apply()
    }

    private fun sendRegistrationToBridge(context: Context) {
        val prefs = getPrefs(context)
        val endpoint = prefs.getString(KEY_ENDPOINT, null)
        val bridgeUrl = prefs.getString(KEY_BRIDGE_URL, null)
        val firebaseAppId = prefs.getString(KEY_FIREBASE_APP_ID, null)
        val firebaseProjectId = prefs.getString(KEY_FIREBASE_PROJECT_ID, null)
        val firebaseApiKey = prefs.getString(KEY_FIREBASE_API_KEY, null)
        val certSha1 = prefs.getString(KEY_CERT_SHA1, null)
        val installId = installId(prefs)
        val installSecret = installSecret(prefs)

        if (endpoint == null || bridgeUrl == null) {
            Log.d(TAG, "Missing data for bridge registration")
            return
        }

        if (firebaseAppId == null || firebaseProjectId == null || firebaseApiKey == null) {
            Log.w(TAG, "Missing Firebase credentials - bridge won't be able to receive FCM")
        }

        val packageName = context.packageName

        // Get app version info - computed upfront as final vals to avoid Ref$ObjectRef
        val packageInfo = try { context.packageManager.getPackageInfo(packageName, 0) } catch (e: Exception) { null }
        val applicationInfo = try { context.packageManager.getApplicationInfo(packageName, 0) } catch (e: Exception) { null }

        val appVersion: Int? = if (packageInfo != null) {
            if (Build.VERSION.SDK_INT >= 28) {
                packageInfo.longVersionCode.toInt()
            } else {
                @Suppress("DEPRECATION")
                packageInfo.versionCode
            }
        } else null

        val appVersionName: String? = packageInfo?.versionName
        val targetSdk: Int? = applicationInfo?.targetSdkVersion

        if (appVersion != null) {
            Log.d(TAG, "App info: version=$appVersion, versionName=$appVersionName, targetSdk=$targetSdk")
        }

        val jsonObj = JSONObject()
        jsonObj.put("endpoint", endpoint)
        jsonObj.put("app_id", packageName)
        jsonObj.put("install_id", installId)
        if (firebaseAppId != null) jsonObj.put("firebase_app_id", firebaseAppId)
        if (firebaseProjectId != null) jsonObj.put("firebase_project_id", firebaseProjectId)
        if (firebaseApiKey != null) jsonObj.put("firebase_api_key", firebaseApiKey)
        if (certSha1 != null) jsonObj.put("cert_sha1", certSha1)
        if (appVersion != null) jsonObj.put("app_version", appVersion)
        if (appVersionName != null) jsonObj.put("app_version_name", appVersionName)
        if (targetSdk != null) jsonObj.put("target_sdk", targetSdk)

        val requestBody = jsonObj.toString()
        val registrationSignature = bridgeUrl + "\n" + requestBody
        val lastSignature = prefs.getString(KEY_LAST_REGISTRATION_SIGNATURE, null)
        if (lastSignature != null
            && lastSignature.equals(registrationSignature)
            && !bridgeHeartbeatDue(prefs)
        ) return

        executor.execute {
            try {
                val url = URL("$bridgeUrl/register")
                val conn = url.openConnection() as HttpURLConnection
                conn.requestMethod = "POST"
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.setRequestProperty("Authorization", "Bearer $installSecret")
                conn.connectTimeout = 10000
                conn.readTimeout = 10000

                val writer = OutputStreamWriter(conn.outputStream)
                writer.write(requestBody)
                writer.flush()
                writer.close()

                val responseCode = conn.responseCode
                if (responseCode == 200) {
                    val reader = BufferedReader(InputStreamReader(conn.inputStream))
                    val sb = StringBuilder()
                    var line: String? = reader.readLine()
                    while (line != null) {
                        sb.append(line)
                        line = reader.readLine()
                    }
                    reader.close()

                    val responseBody = sb.toString()
                    try {
                        val response = JSONObject(responseBody)
                        val bridgeFcmToken = response.optString("fcm_token", null)
                        if (notEmpty(bridgeFcmToken)) {
                            prefs.edit().putString(KEY_BRIDGE_FCM_TOKEN, bridgeFcmToken).apply()
                            Log.i(TAG, "Got bridge FCM token: ${preview(bridgeFcmToken)}")

                            deliverToken(context, bridgeFcmToken)
                        }
                        prefs.edit()
                            .putString(KEY_LAST_REGISTRATION_SIGNATURE, registrationSignature)
                            .putLong(KEY_LAST_REGISTERED_MS, System.currentTimeMillis())
                            .apply()
                        // Must stay last: a try block whose tail expression is Unit-typed
                        // makes Kotlin emit `sget-object kotlin/Unit.INSTANCE`, and the DEX
                        // bundles no Kotlin stdlib, so that read dies on a Java-only host app.
                        Log.i(TAG, "Registered with bridge: ${response.optString("message", "success")}")
                    } catch (e: Exception) {
                        Log.w(TAG, "Could not parse bridge response: $responseBody")
                    }
                } else {
                    val reader = BufferedReader(InputStreamReader(conn.errorStream))
                    val sb = StringBuilder()
                    var line: String? = reader.readLine()
                    while (line != null) {
                        sb.append(line)
                        line = reader.readLine()
                    }
                    reader.close()
                    Log.e(TAG, "Bridge registration failed: $responseCode - $sb")
                }
            } catch (e: Exception) {
                Log.e(TAG, "Bridge registration error", e)
            }
        }
    }

    private fun sendUnregisterToBridge(context: Context) {
        val prefs = getPrefs(context)
        val bridgeUrl = prefs.getString(KEY_BRIDGE_URL, null) ?: return
        val installId = prefs.getString(KEY_INSTALL_ID, null) ?: return
        val installSecret = prefs.getString(KEY_INSTALL_SECRET, null) ?: return
        val packageName = context.packageName

        executor.execute {
            try {
                val url = URL("$bridgeUrl/unregister")
                val conn = url.openConnection() as HttpURLConnection
                conn.requestMethod = "POST"
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.setRequestProperty("Authorization", "Bearer $installSecret")
                conn.connectTimeout = 10000
                conn.readTimeout = 10000

                val jsonObj = JSONObject()
                jsonObj.put("app_id", packageName)
                jsonObj.put("install_id", installId)

                val writer = OutputStreamWriter(conn.outputStream)
                writer.write(jsonObj.toString())
                writer.flush()
                writer.close()

                Log.i(TAG, "Bridge unregister returned ${conn.responseCode}")
            } catch (e: Exception) {
                Log.e(TAG, "Bridge unregister error", e)
            }
        }
    }

    /**
     * Hand a token to the app as if Google had delivered it.
     *
     * The broadcast is explicit and in-process. FirebaseInstanceIdReceiver is exported but
     * guarded by com.google.android.c2dm.permission.SEND, which is owned by GMS, microG, or
     * (on GrapheneOS) app.grapheneos.gmscompat, and cannot be acquired by a third party.
     * Android skips that check when caller and target share an app UID, which is the only
     * permission-independent delivery path and the reason the shim lives inside the APK.
     */
    private fun deliverToken(context: Context, bridgeToken: String) {
        val receiver = getPrefs(context).getString(KEY_C2DM_RECEIVER, null)
        if (!notEmpty(receiver)) {
            Log.w(TAG, "No c2dm receiver configured, cannot deliver token")
            return
        }

        try {
            val intent = Intent(ACTION_NEW_TOKEN)
            intent.setClassName(context.packageName, receiver!!)
            intent.putExtra(EXTRA_TOKEN, bridgeToken)
            context.sendBroadcast(intent)
            lastDeliveryMs = System.currentTimeMillis()
            Log.i(TAG, "Broadcast NEW_TOKEN to $receiver: ${preview(bridgeToken)}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to broadcast NEW_TOKEN", e)
        }
    }

    private fun getPrefs(context: Context): SharedPreferences {
        return context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    @JvmStatic
    fun getEndpoint(context: Context): String? {
        return getPrefs(context).getString(KEY_ENDPOINT, null)
    }

    @JvmStatic
    fun getFcmToken(context: Context): String? {
        return getPrefs(context).getString(KEY_FCM_TOKEN, null)
    }

    @JvmStatic
    fun getBridgeUrl(context: Context): String? {
        return getPrefs(context).getString(KEY_BRIDGE_URL, null)
    }



    private fun mapToJson(map: Map<String, String>): String {
        val sb = StringBuilder("{")
        var first = true
        for ((k, v) in map) {
            if (!first) sb.append(",")
            first = false
            sb.append("\"").append(escapeJson(k)).append("\":\"").append(escapeJson(v)).append("\"")
        }
        sb.append("}")
        return sb.toString()
    }

    private fun escapeJson(s: String): String {
        val sb = StringBuilder()
        for (c in s) {
            when (c) {
                '\\' -> sb.append("\\\\")
                '"' -> sb.append("\\\"")
                '\n' -> sb.append("\\n")
                '\r' -> sb.append("\\r")
                '\t' -> sb.append("\\t")
                else -> sb.append(c)
            }
        }
        return sb.toString()
    }
}
