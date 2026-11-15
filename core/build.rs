//! Linux/macOS select the experimental host behavior model automatically.
//! Bare-metal models still select a complete SDK-owned image, not a Cargo
//! feature or a user-supplied critical-section implementation. Their final link
//! additionally requires the strong ownership anchor and owned vector/trap script.
//! Automatic host selection does not establish bounded waiting or qualification.
fn main() {
    if matches!(std::env::var("TARGET").as_deref(), Ok("aarch64-apple-darwin" | "aarch64-unknown-linux-gnu" | "x86_64-unknown-linux-gnu")) {
        build_hpr3_atomic();
    }
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

use std::{env, path::PathBuf, process::Command};

fn build_hpr3_atomic() {
    let target = env::var("TARGET").expect("Cargo TARGET");
    let host = env::var("HOST").expect("Cargo HOST");
    assert_eq!(target, host, "PAL C compiler requires a native build in this candidate");
    if target == "x86_64-unknown-linux-gnu" {
        // CPUID describes the executing CPU, not all CPUs available after
        // migration. Require CX16 in the executable's minimum ISA profile;
        // the deployment must meet that target on every eligible CPU, just as
        // for any compiler target feature. The C probe is only a sanity check.
        let features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
        assert!(features.split(',').any(|feature| feature == "cmpxchg16b"),
            "Linux x86-64 PAL requires a CX16 minimum-ISA build: use -C target-feature=+cmpxchg16b (or a target-cpu that includes it), and run only on systems providing CX16 on every CPU eligible for this process; local CPUID cannot establish that deployment contract");
    }
    let (base, wide): (&[&str], &[&str]) = match target.as_str() {
        "aarch64-apple-darwin" => (&["-arch", "arm64", "-mmacosx-version-min=11.0"], &[]),
        "x86_64-unknown-linux-gnu" => (&[], &["-mcx16"]),
        "aarch64-unknown-linux-gnu" => (&[], &["-march=armv8-a+lse"]),
        _ => panic!("unsupported wide atomic target"),
    };
    let compiler = env::var_os("CC").unwrap_or_else(|| "clang".into());
    let archiver = env::var_os("AR").unwrap_or_else(|| "ar".into());
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let mut objects = Vec::new();
    for (file, flags) in [("atomic128", wide), ("atomic128_admission", &[][..])] {
        let object = out.join(format!("{file}.o"));
        let status = Command::new(&compiler).args(base).args(flags)
            .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c"])
            .arg(format!("src/{file}.c")).arg("-o").arg(&object)
            .status().expect("Clang required");
        assert!(status.success(), "PAL C compilation rejected");
        println!("cargo:rerun-if-changed=src/{file}.c");
        objects.push(object);
    }
    let archive = out.join("libhpr3_atomic128.a");
    let status = Command::new(archiver).arg("crs").arg(&archive).args(objects)
        .status().expect("ar required");
    assert!(status.success(), "atomic128 archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=hpr3_atomic128");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
}
