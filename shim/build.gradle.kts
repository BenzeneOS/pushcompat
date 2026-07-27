plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.benzeneos.pushcompat.shim"
    compileSdk = 36

    buildToolsVersion = "36.0.0"

    defaultConfig {
        minSdk = 21

        consumerProguardFiles("consumer-rules.pro")
    }

    // Disable androidTest - not needed and breaks Nix dep fetching
    @Suppress("UnstableApiUsage")
    testOptions {
        unitTests.all { it.enabled = false }
    }

    // Don't build test variants
    variantFilter {
        if (name.contains("androidTest", ignoreCase = true)) {
            ignore = true
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false  // Don't minify - avoid class name conflicts
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    kotlinOptions {
        jvmTarget = "1.8"
        // Disable Kotlin intrinsics to avoid runtime dependency on kotlin-stdlib
        freeCompilerArgs = listOf(
            "-Xno-param-assertions",
            "-Xno-call-assertions",
            "-Xno-receiver-assertions"
        )
    }

    // Disable lint - we don't need it and it pulls in dynamic deps that break Nix
    lint {
        checkReleaseBuilds = false
        abortOnError = false
    }
}

dependencies {
    compileOnly("androidx.core:core-ktx:1.12.0")
}

// Configuration for lint dependencies needed at build time
val lintDeps by configurations.creating {
    isCanBeResolved = true
}

dependencies {
    // Fetch lint-gradle so it's available for extractAnnotations task
    lintDeps("com.android.tools.lint:lint-gradle:31.7.3")
}

// Override nixDownloadDeps to skip androidTest configurations (for Nix builds)
afterEvaluate {
    // Disable annotation extraction tasks that pull in lint dynamically
    tasks.matching { it.name.contains("extractAnnotations", ignoreCase = true) }.configureEach {
        enabled = false
    }

    tasks.findByName("nixDownloadDeps")?.let { task ->
        task.actions.clear()
        task.doLast {
            configurations
                .filter { it.isCanBeResolved }
                .filter { !it.name.contains("AndroidTest", ignoreCase = true) }
                .filter { !it.name.contains("UnitTest", ignoreCase = true) }
                .forEach {
                    try {
                        it.resolve()
                    } catch (e: Exception) {
                        logger.warn("Skipping unresolvable config: ${it.name}")
                    }
                }
            buildscript.configurations
                .filter { it.isCanBeResolved }
                .forEach { it.resolve() }
        }
    }
}
