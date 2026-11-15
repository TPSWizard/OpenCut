// ConzaCut for Android: a Trusted Web Activity that opens the web editor
// (https://conzacut.conzahub.com) full screen in the phone's Chrome, so it
// has WebCodecs, the file pickers and downloads the editor needs. There is
// no app code: the launcher activity comes from androidbrowserhelper.
//
// Release builds are signed with the key in ANDROID_KEYSTORE_PATH (the
// GitHub workflow decodes it from a secret); that key's SHA-256 is listed in
// apps/web/public/.well-known/assetlinks.json, which is what lets Chrome
// drop the address bar. Without the key, release falls back to the debug
// key: the app works, with an address bar.

plugins {
    id("com.android.application")
}

val keystorePath: String? = System.getenv("ANDROID_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }

android {
    namespace = "com.conzahub.conzacut"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.conzahub.conzacut"
        minSdk = 24
        targetSdk = 36
        versionCode = (project.findProperty("versionCode") as String?)?.toInt() ?: 1
        versionName = (project.findProperty("versionName") as String?) ?: "1.0.0"
    }

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS") ?: "conzacut"
                keyPassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig =
                if (keystorePath != null) signingConfigs.getByName("release")
                else signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("com.google.androidbrowserhelper:androidbrowserhelper:2.7.4")
}
