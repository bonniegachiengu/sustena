plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "online.vyybandasky.sustena.smscapture"
    compileSdk = 36
    defaultConfig {
        minSdk = 24
    }
    kotlinOptions { jvmTarget = "1.8" }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation(project(":tauri-android"))

    // ★★★ The two filters decide what leaves the handset, and until now
    //     nothing exercised them. They are pure functions over a string, so
    //     they need no device and no emulator -- a plain JVM test is the whole
    //     cost, and the alternative was shipping the one part of this feature
    //     that must never be wrong on the strength of reading it.
    testImplementation("junit:junit:4.13.2")
}
