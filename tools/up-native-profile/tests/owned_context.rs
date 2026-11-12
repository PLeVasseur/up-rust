//! Serialized declaration CLI admission; no native inventory or nested Cargo.
use std::process::Command;
#[test]
fn verify_owned_context_reads_distinct_provisioned_files_and_rejects_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("local.json");
    let peer = dir.path().join("peer.json");
    // Equal literal declarations are fixtures, not actual peer deployment evidence.
    std::fs::write(&local, include_str!("fixtures/door-json.json")).unwrap();
    std::fs::write(&peer, include_str!("fixtures/door-json.json")).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_up-native-profile"))
            .arg("verify-owned-context")
            .arg("--local")
            .arg(&local)
            .arg("--peer")
            .arg(&peer)
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let mut mismatch: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/door-json.json")).unwrap();
    *mismatch
        .get_mut("payload_family")
        .unwrap()
        .get_mut("literal")
        .unwrap() = serde_json::json!("example.incompatible.family");
    std::fs::write(&peer, serde_json::to_vec(&mismatch).unwrap()).unwrap();
    assert!(!run().status.success());
}
