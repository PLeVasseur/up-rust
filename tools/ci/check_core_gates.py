#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# See the NOTICE file(s) distributed with this work for copyright ownership.
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0
"""Mechanical gates for up-rust-core (R34, R48).

Each gate fails loudly. Negative controls prove a gate can fail.
"""
from __future__ import annotations

import json
import argparse
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tomllib

COMMAND_LOG = None
COMMAND_INDEX = 0

ROOT = pathlib.Path(__file__).resolve().parents[2]
CORE = ROOT / "core"
FORBIDDEN_TOKENS = ("core::task", "core::future", "core::pin", "alloc::", "extern crate alloc", "extern crate std", "std::")

# Preparatory mapping: req~ssr-atomic-width~1 at neutral spec 733b006.
# Only facade routing is enforced here; the width/protocol port and G3 are pending.
ATOMIC_FACADE = pathlib.PurePosixPath("core/src/platform/atomic.rs")


def rust_tokens(source: str) -> list[tuple[str, int]]:
    """Tokens/offsets for the import gate, excluding comments and string literals.

    This is a source gate, not Rust name resolution or macro expansion. Ancestor
    imports/aliases are refused so spelling an atomic through an alias cannot
    bypass it. Nested comments, raw identifiers and multiline use trees count.
    """
    tokens = []
    pattern = re.compile(r'//[^\n]*|/\*|(?:br|cr|r)(\#*)"|[bc]?"(?:\\.|[^"\\])*"|'
                         r"b?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\])'|"
                         r'(?:r#)?[A-Za-z_][A-Za-z_0-9]*|::|[^\s]', re.DOTALL)
    pos = 0
    while match := pattern.search(source, pos):
        token = match[0]
        pos = match.end()
        if token == "/*":
            depth = 1
            while depth and pos < len(source):
                marker = re.search(r'/\*|\*/', source[pos:])
                if marker is None:
                    pos = len(source)
                    break
                depth += 1 if marker[0] == "/*" else -1
                pos += marker.end()
        elif match[1] is not None:
            end = source.find('"' + match[1], pos)
            pos = len(source) if end < 0 else end + 1 + len(match[1])
        elif token.startswith(("//", '"', 'b"', 'c"', "'", "b'")):
            continue
        else:
            tokens.append((token.removeprefix("r#"), match.start()))
    return tokens


def atomic_import_violations(source: str, path: pathlib.PurePath) -> list[str]:
    """Forbid atomic paths and alias/glob imports of their ancestors outside the facade."""
    if pathlib.PurePosixPath(path.as_posix()) == ATOMIC_FACADE:
        return []
    tokens = rust_tokens(source)
    words = [token for token, _ in tokens]
    hits = set()

    def forbidden(parts: list[str]) -> bool:
        return bool(parts and parts[0] in ("core", "std") and
                    (len(parts) == 1 or parts[1] in ("sync", "*")))

    def use_tree(i: int, prefix: list[str], origin: int) -> int:
        parts = list(prefix)
        while i < len(words):
            word = words[i]
            if word == "::":
                i += 1
            elif word == "{":
                i += 1
                while i < len(words) and words[i] != "}":
                    if words[i] == ",":
                        i += 1
                    else:
                        next_i = use_tree(i, parts, origin)
                        # Macro token trees can contain `use` without an import.
                        # Abandon this parse if a child cannot consume a token;
                        # the outer scan still checks every later `use`/path.
                        if next_i <= i:
                            return i
                        i = next_i
                return i + 1
            elif word in ("as", ",", ";", "}"):
                if forbidden(parts):
                    hits.add(origin)
                return i + 2 if word == "as" else i
            else:
                if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*|\*", word):
                    return i
                if word != "self":
                    parts.append(word)
                i += 1
        return i

    for i, word in enumerate(words):
        if word == "use":
            use_tree(i + 1, [], i)
        elif word in ("core", "std") and words[i + 1:i + 3] == ["::", "sync"]:
            hits.add(i)
        elif word == "extern" and words[i + 1:i + 2] == ["crate"] and words[i + 2:i + 3] in (["core"], ["std"]):
            hits.add(i)
    return [f"{source.count(chr(10), 0, tokens[i][1]) + 1}: atomic import bypasses platform::atomic"
            for i in sorted(hits)]


# R59's exact shipped seam. A new name needs an explicit design decision.
ASYNC_SEAM = {"__route_wake", "__wake", "__ttl", "__taken_value", "__taken_payload"}

def hook_unresolved(name: str, returncode: int, stderr: str) -> bool:
    """Accept only a primary missing-value/function diagnostic for this exact hook."""
    diagnostic = re.compile(
        r"^error\[(?:E0425|E0599)\]: "
        r"(?:cannot find (?:value|function) `(?P<free>[^`]+)`|"
        r"no (?:associated item|function or associated item|associated function or constant) "
        r"named `(?P<associated>[^`]+)` found)", re.MULTILINE)
    return returncode != 0 and any(
        (match.group("free") or match.group("associated")) == name
        for match in diagnostic.finditer(stderr))

def hook_violations(source: str) -> list[str]:
    lines = source.splitlines()
    violations = []
    for i, line in enumerate(lines):
        match = re.match(r"\s*pub\s+(?:(?:const|unsafe|async)\s+)*fn\s+(__\w+|startup_values)\b", line)
        if not match or match[1] in ASYNC_SEAM:
            continue
        j = i - 1
        attributes = []
        while j >= 0:
            text = lines[j].strip()
            if text.startswith(("#[", "//")) or not text:
                attributes.append(text)
                j -= 1
            else:
                break
        if "#[cfg(up_rust_test_hooks)]" not in attributes:
            violations.append(f"{i + 1}: {match[1]}")
    return violations


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd), flush=True)
    global COMMAND_INDEX
    result = subprocess.run(cmd, text=True, capture_output=True, check=False, **kwargs)
    if COMMAND_LOG is not None:
        COMMAND_INDEX += 1
        prefix = COMMAND_LOG / str(COMMAND_INDEX)
        prefix.with_suffix(".stdout").write_text(result.stdout)
        prefix.with_suffix(".stderr").write_text(result.stderr)
        prefix.with_suffix(".json").write_text(json.dumps({"argv": cmd, "exit": result.returncode}))
    return result


class Gates:
    def __init__(self, toolchain: str, target_dir: pathlib.Path) -> None:
        global COMMAND_LOG
        COMMAND_LOG = target_dir / "observations" / str(os.getpid())
        COMMAND_LOG.mkdir(parents=True, exist_ok=False)
        self.toolchain = toolchain
        self.rustc = ["rustc", f"+{toolchain}"]
        self.target_dir = target_dir
        self.host = self._print("host")
        self.failures = 0

    def _print(self, what: str) -> str:
        output = run(self.rustc + ["-vV"]).stdout
        for line in output.splitlines():
            if line.startswith(f"{what}: "):
                return line.split(": ", 1)[1].strip()
        raise SystemExit(f"cannot determine {what}")

    def fail(self, name: str, detail: str = "") -> None:
        self.failures += 1
        print(f"FAIL {name}\n{detail}", flush=True)

    def ok(self, name: str, detail: str = "") -> None:
        print(f"PASS {name} {detail}".rstrip(), flush=True)

    # 1. The manifest stays dependency-free and feature-free.
    def manifest(self) -> None:
        data = tomllib.loads((CORE / "Cargo.toml").read_text(encoding="utf-8"))
        forbidden = [key for key in ("features", "dependencies", "build-dependencies", "dev-dependencies", "target") if key in data]
        if forbidden:
            self.fail("manifest", f"core/Cargo.toml has forbidden sections: {forbidden}")
        else:
            self.ok("manifest", "no [features], [dependencies], [build-dependencies], [dev-dependencies] or [target]")

    # 2. The certified surface excludes these modules; alloc and std cannot appear.
    def source_scan(self) -> None:
        hits: list[str] = []
        for path in sorted((CORE / "src").rglob("*.rs")):
            source = path.read_text(encoding="utf-8")
            hits.extend(f"{path.relative_to(ROOT)}:{hit}" for hit in
                        atomic_import_violations(source, path.relative_to(ROOT)))
            for number, line in enumerate(source.splitlines(), 1):
                stripped = line.strip()
                if stripped.startswith("//"):
                    continue
                for token in FORBIDDEN_TOKENS:
                    if token in line:
                        hits.append(f"{path.relative_to(ROOT)}:{number}: {line.strip()}")
        for base in (CORE / "src", ROOT / "src"):
            for path in sorted(base.rglob("*.rs")):
                hits.extend(f"{path.relative_to(ROOT)}:{hit}" for hit in hook_violations(path.read_text()))
        controls = (
            ("pub fn __unlisted() {}", True),
            ("pub const fn __unlisted() {}", True),
            ("#[doc(hidden)]\npub fn __unlisted() {}", True),
            ("#[cfg(up_rust_test_hooks)]\npub fn __unlisted() {}", False),
            ("pub fn __wake() {}", False),
            ("pub fn startup_values() {}", True),
        )
        if any(bool(hook_violations(text)) != expected for text, expected in controls):
            hits.append("test-hook source scan negative/positive control failed")
        if hits:
            self.fail("source-scan", "\n".join(hits))
        else:
            self.ok("source-scan", f"no {', '.join(FORBIDDEN_TOKENS)}")

    def _restricted_sysroot(self, name: str, source: pathlib.Path, extra: list[str] | None = None) -> subprocess.CompletedProcess[str]:
        sysroot = run(self.rustc + ["--print", "sysroot"]).stdout.strip()
        libdir = pathlib.Path(sysroot) / "lib" / "rustlib" / self.host / "lib"
        restricted = self.target_dir / "restricted" / name / "sysroot" / "lib" / "rustlib" / self.host / "lib"
        restricted.mkdir(parents=True, exist_ok=True)
        wanted = re.compile(r"^(libcore-|libcompiler_builtins-|librustc_std_workspace_core-)")
        for entry in libdir.iterdir():
            if wanted.match(entry.name):
                shutil.copy2(entry, restricted / entry.name)
        output = restricted.parents[3] / "out"
        output.mkdir(parents=True, exist_ok=True)
        return run(
            self.rustc
            + [
                "--sysroot",
                str(restricted.parents[3]),
                "--crate-name",
                "up_rust_core",
                "--crate-type",
                "rlib",
                "--edition",
                "2021",
                "--out-dir",
                str(output),
            ]
            + (extra or [])
            + [str(source)]
        )

    # 3. It builds on a sysroot with no alloc or std, and the control proves that sysroot lacks alloc.
    def sysroot(self) -> None:
        result = self._restricted_sysroot("core", CORE / "src" / "lib.rs")
        if result.returncode != 0:
            self.fail("sysroot-only-core", result.stderr[-2000:])
        else:
            self.ok("sysroot-only-core", "builds with only core, compiler_builtins and rustc_std_workspace_core")
        control = self.target_dir / "control_alloc.rs"
        control.parent.mkdir(parents=True, exist_ok=True)
        control.write_text("#![no_std]\nextern crate alloc;\npub fn f() -> alloc::string::String { alloc::string::String::new() }\n", encoding="utf-8")
        result = self._restricted_sysroot("control", control)
        if result.returncode == 0 or "can't find crate for `alloc`" not in result.stderr:
            self.fail("sysroot-control-alloc", "the restricted sysroot unexpectedly provides alloc:\n" + result.stderr[-1000:])
        else:
            self.ok("sysroot-control-alloc", "extern crate alloc fails on the restricted sysroot")

    # The derive macro the two-mode fixture needs, built once for the host.
    def _macros(self) -> str | None:
        if getattr(self, "_macro_path", None) is not None:
            return self._macro_path
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "macros")
        result = run(["cargo", f"+{self.toolchain}", "build", "--offline", "--locked", "-p", "up-rust-macros", "--message-format=json"], cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("panic-link-macros", result.stderr[-3000:])
            return None
        for line in result.stdout.splitlines():
            if '"up_rust_macros"' in line and '"filenames"' in line:
                data = __import__("json").loads(line)
                for name in data.get("filenames", []):
                    if name.endswith((".so", ".dylib")):
                        self._macro_path = name
                        return name
        self.fail("panic-link-macros", "no proc-macro artifact found")
        return None

    # 4. Link-time no-panic check, plus a symbol check on the linked image.
    def _build_fixture(self, inject: bool, fixture: str = "panic_free") -> tuple[subprocess.CompletedProcess[str], pathlib.Path]:
        name = ("inject" if inject else "clean") + ("" if fixture == "panic_free" else "-two-mode")
        out = self.target_dir / "panic-link" / name
        out.mkdir(parents=True, exist_ok=True)
        flags = ["--edition", "2021", "-C", "opt-level=3", "-C", "panic=abort", "-C", "codegen-units=1", "-C", "overflow-checks=on", "-C", "debuginfo=0"]
        rlib = out / "libup_rust_core.rlib"
        built = run(self.rustc + ["--crate-name", "up_rust_core", "--crate-type", "rlib"] + flags + ["-o", str(rlib), str(CORE / "src" / "lib.rs")])
        if built.returncode != 0:
            return built, out
        # An undefined symbol must fail the link so a surviving panic path is caught:
        # the macOS linker refuses them by default; on Linux a shared object needs the -z defs linker flag.
        link_args = (
            ["-C", "link-arg=-lSystem"]
            if platform.system() == "Darwin"
            else ["-C", "link-arg=-Wl,-z,defs", "-C", "link-arg=-lc"]
        )
        cmd = self.rustc + ["--crate-name", "panic_free", "--crate-type", "cdylib"] + flags + ["-C", "lto=fat"] + link_args + ["--extern", f"up_rust_core={rlib}", "-o", str(out / "libpanic_free.so")]
        if fixture != "panic_free":
            macros = self._macros()
            if macros is None:
                return subprocess.CompletedProcess(cmd, 1, "", "the macro crate did not build"), out
            cmd += ["--extern", f"up_rust_macros={macros}"]
        if inject:
            cmd += ["--cfg", "inject_panic"]
        cmd += [str(CORE / "verify" / f"{fixture}.rs")]
        return run(cmd), out

    def _check_panic_fixture(self, fixture: str) -> None:
        result, out = self._build_fixture(inject=False, fixture=fixture)
        if result.returncode != 0:
            self.fail(f"panic-link[{fixture}]", result.stdout[-2000:] + result.stderr[-4000:])
        else:
            self.ok(f"panic-link[{fixture}]", "no panic path survived link-time optimization (overflow checks on)")
            nm = run(["nm", "-u", str(out / "libpanic_free.so")])
            print(nm.stdout, flush=True)
            bad = re.findall(r"\b(__rust_alloc\w*|__rust_dealloc\w*|__rust_realloc\w*|malloc|calloc|realloc|free|_Unwind_\w+|__rust_panic\w*)\b", nm.stdout)
            if nm.returncode != 0 or bad:
                self.fail(f"no-heap-symbols[{fixture}]", f"linked image references: {bad}\n{nm.stderr}")
            else:
                self.ok(f"no-heap-symbols[{fixture}]", "the linked image references no allocator or unwinder")
        result, _ = self._build_fixture(inject=True, fixture=fixture)
        combined = result.stdout + result.stderr
        if result.returncode == 0 or "__up_rust_core_panic_path_survived" not in combined:
            self.fail(f"panic-link-control[{fixture}]", "an injected panic path was not caught:\n" + combined[-2000:])
        else:
            self.ok(f"panic-link-control[{fixture}]", "an injected panic path fails the link, naming the trap symbol")

    def panic_link(self) -> None:
        self._check_panic_fixture("panic_free")
        self._check_panic_fixture("panic_free_two_mode")

    # 5. It builds for a bare-metal target (no Native backend there), and that
    # target must have 64-bit atomics: `FreeIndices` and `CallSlot` need them.
    def bare_metal(self) -> None:
        targets = run(["rustup", "target", "list", "--installed", "--toolchain", self.toolchain]).stdout.split()
        candidates = ["aarch64-unknown-none", "x86_64-unknown-none", "riscv64gc-unknown-none-elf"]
        chosen = next((t for t in candidates if t in targets), None)
        if chosen is None:
            self.fail("bare-metal", f"no bare-metal target with 64-bit atomics is installed for {self.toolchain}: tried {candidates}")
            return
        atomics = run(self.rustc + ["--print", "cfg", "--target", chosen]).stdout
        if 'target_has_atomic="64"' not in atomics:
            self.fail("bare-metal", f"{chosen} has no 64-bit atomics")
            return
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "bare-metal")
        result = run(["cargo", f"+{self.toolchain}", "build", "--offline", "--locked", "-p", "up-rust-core", "--target", chosen], cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("bare-metal", result.stderr[-3000:])
        else:
            self.ok("bare-metal", f"builds for {chosen}")


    # The QNX backend is source only, but it must at least type-check.
    def qnx_typecheck(self) -> None:
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "qnx-typecheck")
        env["RUSTFLAGS"] = "--cfg check_qnx_backend"
        result = run(["cargo", f"+{self.toolchain}", "check", "--offline", "--locked", "-p", "up-rust-core"], cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("qnx-typecheck", result.stderr[-3000:])
        else:
            self.ok("qnx-typecheck", "the source-only QNX backend type-checks on this host")

    # 6. A crate depending only on up-rust-core and up-rust-macros compiles, with the
    # derives on the core-only path.
    def consumer(self) -> None:
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(self.target_dir / "consumer")
        env["RUSTFLAGS"] = ""
        env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        base = ["cargo", f"+{self.toolchain}", "check", "--offline", "--locked", "-p", "up-rust-core-consumer"]
        result = run(base, cwd=ROOT, env=env)
        if result.returncode != 0:
            self.fail("consumer", result.stderr[-3000:])
        elif self.shipped_hooks(env):
            self.ok("consumer", "core consumer and shipped SDK/Tokio compile without hooks; all test hooks fail to resolve; URI warnings name replacements")


    def shipped_hooks(self, env: dict[str, str]) -> bool:
        command = ["cargo", f"+{self.toolchain}", "build", "--offline", "--locked",
                   "-p", "up-rust", "-p", "up-rust-tokio",
                   "--features", "up-rust/test-util,up-rust/zero-copy-transport",
                   "--message-format=json"]
        result = run(command, cwd=ROOT, env=env)
        if result.returncode:
            self.fail("consumer", result.stdout[-12000:] + result.stderr[-4000:])
            return False
        libraries = {}
        for line in result.stdout.splitlines():
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if item.get("reason") == "compiler-artifact":
                for name in item.get("filenames", []):
                    if name.endswith(".rlib"):
                        libraries[item["target"]["name"]] = name
        out = self.target_dir / "hook-probes"
        out.mkdir(parents=True, exist_ok=True)
        probes = {
            "__free_value_slot": "up_rust_core::communication::two_mode::Publisher::<u32>::__free_value_slot::<()>",
            "__claim_first_entry": "up_rust_core::transport::inbound::PeerTable::__claim_first_entry",
            "__slice_for_tests": "up_rust_core::memory::StartupMemory::__slice_for_tests::<u32>",
            "__admit_for_tests": "up_rust_core::transport::envelope::__admit_for_tests",
            "__encode_head_for_tests": "up_rust_core::transport::envelope::__encode_head_for_tests",
            "__parse_for_tests": "up_rust_core::transport::envelope::__parse_for_tests",
            "__contract_slot": "up_rust_core::wire::__contract_slot::<u32>",
            "__contract_value": "up_rust_core::wire::__contract_value::<u32>",
            "__abi_offsets": "up_rust_core::uri::UUri::__abi_offsets",
            "__resolve_type_identity_for_test": "up_rust::NativeProfileAgreement::__resolve_type_identity_for_test::<u32, up_rust::NativeProfileError>",
            "startup_values": "up_rust::two_mode::startup_values",
        }
        args = self.rustc + ["--edition=2021", "--crate-type=lib", "--emit=metadata",
                            "-L", "dependency=" + str(pathlib.Path(env["CARGO_TARGET_DIR"]) / "debug" / "deps")]
        for name in ("up_rust", "up_rust_core"):
            args += ["--extern", name + "=" + libraries[name]]
        success = True
        for name, expression in probes.items():
            source = out / (name + ".rs")
            source.write_text(f"pub fn probe() {{ let _ = {expression}; }}\n")
            probe = run(args + ["--crate-name", "probe", "-o", str(out / (name + ".rmeta")), str(source)], env=env)
            (out / (name + ".stderr")).write_text(probe.stderr)
            if not hook_unresolved(name, probe.returncode, probe.stderr):
                self.fail("consumer", f"hook {name} did not fail specifically at resolution:\n" + probe.stderr)
                success = False
        for crate in ("up_rust_core::uri", "up_rust"):
            name = crate.replace("::", "_") + "_uri_aliases"
            source = out / (name + ".rs")
            source.write_text(f"pub fn probe(uri: &{crate}::UUri) {{ let _ = (uri.ue_type_id(), uri.ue_instance_id()); let _ = (uri.uentity_type_id(), uri.uentity_instance_id()); }}\n")
            probe = run(args + ["--crate-name", "probe", "-o", str(out / (name + ".rmeta")), str(source)], env=env)
            (out / (name + ".stderr")).write_text(probe.stderr)
            if probe.returncode or not all("use " + method + " instead" in probe.stderr for method in ("ue_type_id", "ue_instance_id")):
                self.fail("consumer", "URI deprecation compile proof failed:\n" + probe.stderr)
                success = False
        return success


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", default="stable")
    parser.add_argument("--target-dir", default=os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    parser.add_argument("--only", choices=["manifest", "source_scan", "sysroot", "panic_link", "bare_metal", "qnx_typecheck", "consumer"])
    args = parser.parse_args()
    gates = Gates(args.toolchain, pathlib.Path(args.target_dir) / "core-gates" / args.toolchain)
    for name in ("manifest", "source_scan", "sysroot", "panic_link", "bare_metal", "qnx_typecheck", "consumer"):
        if args.only is None or args.only == name:
            getattr(gates, name)()
    print(f"core gates on {args.toolchain}: {gates.failures} failure(s)", flush=True)
    return 1 if gates.failures else 0


if __name__ == "__main__":
    sys.exit(main())
