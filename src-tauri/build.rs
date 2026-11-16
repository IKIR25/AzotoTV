fn main() {
    // The Android app is built by android/build.sh and embedded by phone.rs.
    // It is not in the repo: without it the build still works and the
    // download answers 404.
    let apk = std::path::Path::new("phone/AzotoTV.apk");
    if !apk.exists() {
        let _ = std::fs::write(apk, b"");
    }
    println!("cargo:rerun-if-changed=phone/AzotoTV.apk");
    tauri_build::build()
}
