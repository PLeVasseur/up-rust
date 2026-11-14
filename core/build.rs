//! Linux/macOS select the experimental host behavior model automatically.
//! Bare-metal models still select a complete SDK-owned image, not a Cargo
//! feature or a user-supplied critical-section implementation. Their final link
//! additionally requires the strong ownership anchor and owned vector/trap script.
//! Automatic host selection does not establish bounded waiting or qualification.
fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_CFG_UP_RUST_NATIVE32_MODEL");
    println!("cargo:rustc-check-cfg=cfg(up_rust_native32_model, values(\"host\", \"cortex-m7-an500\", \"riscv32-sifive-e\"))");
    let target = std::env::var("TARGET").expect("Cargo supplies TARGET");
    let os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo supplies target OS");
    let model = match std::env::var("CARGO_CFG_UP_RUST_NATIVE32_MODEL") {
        Ok(explicit) => explicit,
        Err(std::env::VarError::NotPresent) if os == "macos" || os == "linux" => {
            // Emit the selection for this crate, including its unit tests.
            // A dependency's build script does not set flags for its consumers.
            println!("cargo:rustc-cfg=up_rust_native32_model=\"host\"");
            String::from("host")
        }
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(error) => panic!("invalid native32 execution-model environment: {error}"),
    };
    let allowed = match model.as_str() {
        "cortex-m7-an500" => target == "thumbv7em-none-eabihf",
        "riscv32-sifive-e" => target == "riscv32imac-unknown-none-elf",
        "host" => os == "macos" || os == "linux",
        _ => false,
    };
    assert!(allowed,
        "native32 experiment: Linux/macOS automatically select the host behavior model; bare-metal builds must select --cfg up_rust_native32_model=\"cortex-m7-an500\" (thumbv7em-none-eabihf) or \"riscv32-sifive-e\" (riscv32imac-unknown-none-elf); unsupported profile/target pair");
}
