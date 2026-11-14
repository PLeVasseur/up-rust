//! This experiment selects a complete SDK-owned image, not a Cargo feature or a
//! user-supplied critical-section implementation. The final link additionally
//! requires the image's strong ownership anchor and owned vector/trap script.
fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_CFG_UP_RUST_NATIVE32_MODEL");
    println!("cargo:rustc-check-cfg=cfg(up_rust_native32_model, values(\"host\", \"cortex-m7-an500\", \"riscv32-sifive-e\"))");
    let target = std::env::var("TARGET").expect("Cargo supplies TARGET");
    let model = std::env::var("CARGO_CFG_UP_RUST_NATIVE32_MODEL").unwrap_or_default();
    let allowed = match model.as_str() {
        "cortex-m7-an500" => target == "thumbv7em-none-eabihf",
        "riscv32-sifive-e" => target == "riscv32imac-unknown-none-elf",
        "host" => {
            let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
            os == "macos" || os == "linux"
        }
        _ => false,
    };
    assert!(allowed,
        "native32 experiment: select the supplied SDK image with --cfg up_rust_native32_model=\"cortex-m7-an500\" (thumbv7em-none-eabihf), \"riscv32-sifive-e\" (riscv32imac-unknown-none-elf), or explicit \"host\" behavior model; unsupported profile/target pair");
}
