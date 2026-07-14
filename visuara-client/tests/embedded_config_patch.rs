//! Proves the binary-patching mechanism against the actual compiled
//! visuara-client executable, not a synthetic fixture — this is exactly
//! what the signaling server's download endpoint will do to a real release
//! binary.

use visuara_common::embedded_config::EmbeddedConfig;

#[test]
fn patches_and_reads_back_real_binary() {
    let exe_path = env!("CARGO_BIN_EXE_visuara-client");
    let original = std::fs::read(exe_path).expect("read compiled binary");

    let config = EmbeddedConfig {
        server_url: Some("wss://visuara.example.com/ws".to_string()),
        device_name: Some("warehouse-pc-04".to_string()),
    };

    let patched = config.patch_binary(&original).expect("patch real binary");
    assert_eq!(patched.len(), original.len(), "patching must not change file size");

    let read_back = EmbeddedConfig::find_in_binary(&patched)
        .expect("search patched binary")
        .expect("marker should be present in a patched binary");

    assert_eq!(read_back.server_url, config.server_url);
    assert_eq!(read_back.device_name, config.device_name);
}
