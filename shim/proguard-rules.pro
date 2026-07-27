# Keep all public methods of PushCompatShim (called via reflection and smali hooks)
-keep class com.benzeneos.pushcompat.shim.PushCompatShim {
    public static *;
}

# Keep the receiver (referenced in AndroidManifest)
-keep class com.benzeneos.pushcompat.shim.PushCompatReceiver {
    public *;
}

# Keep annotations
-keepattributes *Annotation*
