fn main() {
    // Ensure protoc (Protocol Buffers compiler) is on the PATH for prost-build,
    // which LanceDB's encoding layer requires. If the PROTOC env var is already
    // set or protoc is on the system PATH, this is a no-op.
    #[cfg(target_os = "windows")]
    set_protoc_windows();

    tauri_build::build()
}

/// On Windows, winget installs protoc under the user's WinGet packages folder.
/// Detect it and set PROTOC so prost-build can find it without the developer
/// having to restart their shell or set the variable manually.
#[cfg(target_os = "windows")]
fn set_protoc_windows() {
    if std::env::var("PROTOC").is_ok() {
        return; // already set — nothing to do
    }

    let local_appdata = match std::env::var("LOCALAPPDATA") {
        Ok(v) => std::path::PathBuf::from(v),
        Err(_) => return,
    };

    let winget_base = local_appdata.join("Microsoft\\WinGet\\Packages");
    let pattern = "Google.Protobuf_Microsoft.Winget.Source_8wekyb3d8bbwe";
    let candidate = winget_base.join(pattern).join("bin").join("protoc.exe");

    if candidate.exists() {
        unsafe {
            std::env::set_var("PROTOC", candidate);
        }
    }
}
