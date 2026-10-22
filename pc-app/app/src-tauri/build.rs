fn main() {
    // tauri-build embeds icons/icon.ico in the exe but only reruns when
    // tauri.conf.json changes, so replaced icons would never reach the build.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
