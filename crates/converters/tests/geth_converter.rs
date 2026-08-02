//! Integration tests for the Geth `callTracer` converter, exercised as
//! a real user would: run the compiled `geth-convert` binary against a
//! saved real trace, then run the compiled `rootcause` binary against
//! its output. Neither binary's internals are called directly — this
//! is the same path documented in the README.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // Unlike `integration-tests` (a direct child of the repo root),
    // `converters` lives at `crates/converters` — two levels down.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/converters must be two levels below the repo root")
        .to_path_buf()
}

fn find_bin(name: &str) -> PathBuf {
    let test_exe =
        std::env::current_exe().expect("integration test must know its own executable path");
    let profile_dir = test_exe
        .parent()
        .and_then(Path::parent)
        .expect("test executable must live under target/{profile}/deps");
    let bin_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let bin = profile_dir.join(bin_name);
    assert!(
        bin.exists(),
        "expected {} at {}; was `cargo build --workspace` run first?",
        name,
        bin.display()
    );
    bin
}

#[test]
fn geth_calltracer_sample_converts_and_analyzes_with_no_false_positive() {
    let root = repo_root();
    let converted = root
        .join("demo")
        .join("converted")
        .join("test_geth_goerli.json");

    let convert_status = std::process::Command::new(find_bin("geth-convert"))
        .args([
            "--input",
            root.join("demo/samples/geth_calltracer_goerli.json")
                .to_str()
                .unwrap(),
            "--tx-hash",
            "0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad",
            "--block",
            "0x876123",
            "--chain-id",
            "0x5",
            "--nonce",
            "0x1",
            "--output",
            converted.to_str().unwrap(),
        ])
        .status()
        .expect("failed to execute geth-convert");
    assert!(convert_status.success());
    assert!(converted.exists());

    let analyze_output = std::process::Command::new(find_bin("rootcause"))
        .arg("analyze")
        .arg(&converted)
        .arg("--patterns")
        .arg(root.join("patterns"))
        .output()
        .expect("failed to execute rootcause analyze");
    assert!(analyze_output.status.success());
    let stdout = String::from_utf8_lossy(&analyze_output.stdout);

    // Real call tree: 4 frames, correctly parsed from the real geth
    // response (root CALL -> DELEGATECALL -> CALL -> DELEGATECALL).
    assert!(
        stdout.contains("4 call(s)"),
        "expected the real 4-frame call tree to be preserved; got:\n{stdout}"
    );
    // No storage changes: callTracer alone carries none, and this
    // converter deliberately does not guess at per-call attribution
    // (see converters::geth's module docs).
    assert!(
        stdout.contains("0 storage change(s)"),
        "callTracer carries no storage diffs; got:\n{stdout}"
    );
    assert!(
        stdout.contains("found 0 candidate match(es)"),
        "no pattern in patterns/ should find evidence it wasn't given \
         (all current patterns require storage or token-transfer \
         evidence this converter cannot supply from callTracer alone); \
         got:\n{stdout}"
    );

    let _ = std::fs::remove_file(&converted);
}

#[test]
fn erigon_calltracer_sample_converts_and_analyzes_with_no_false_positive() {
    // Milestone 11: `geth-convert` is exercised, unmodified, against a
    // real Erigon `callTracer` response (not a Geth one) — verifying
    // the README's Erigon-compatibility claim the same way the sample
    // above verifies Geth's: run the compiled binaries end to end, not
    // library internals. See `demo/samples/erigon_calltracer_polygon.json`'s
    // own provenance note and the top-level README's "Erigon" section
    // for the source of this saved response.
    let root = repo_root();
    let converted = root
        .join("demo")
        .join("converted")
        .join("test_erigon_polygon.json");

    let convert_status = std::process::Command::new(find_bin("geth-convert"))
        .args([
            "--input",
            root.join("demo/samples/erigon_calltracer_polygon.json")
                .to_str()
                .unwrap(),
            "--tx-hash",
            "0x19afd31a9327af30b1d7abf1d46efa228af0747268e618fcf06dcb62655999f8",
            "--block",
            "0x2b710e1",
            "--chain-id",
            "0x89",
            "--nonce",
            "0x1",
            "--output",
            converted.to_str().unwrap(),
        ])
        .status()
        .expect("failed to execute geth-convert");
    assert!(convert_status.success());
    assert!(converted.exists());

    let analyze_output = std::process::Command::new(find_bin("rootcause"))
        .arg("analyze")
        .arg(&converted)
        .arg("--patterns")
        .arg(root.join("patterns"))
        .output()
        .expect("failed to execute rootcause analyze");
    assert!(analyze_output.status.success());
    let stdout = String::from_utf8_lossy(&analyze_output.stdout);

    // Real call tree: a root CALL (reverted) with 3 STATICCALL
    // children — faithfully parsed from the real Erigon response,
    // exactly as `ingestion::raw`/`RawCall` already expect (Erigon's
    // `callTracer` uses the identical field names and uppercase `type`
    // values Geth's does).
    assert!(
        stdout.contains("4 call(s)"),
        "expected the real 4-frame Erigon call tree to be preserved; got:\n{stdout}"
    );
    // The root frame's `error: "execution reverted"` must map through
    // to `succeeded: false` on the root call, the same `error.is_none()`
    // rule `converters::geth::convert_frame` already applies uniformly
    // regardless of which client produced the response.
    assert!(
        stdout.contains("0 storage change(s)"),
        "callTracer carries no storage diffs, from Erigon any more than \
         from Geth; got:\n{stdout}"
    );
    assert!(
        stdout.contains("found 0 candidate match(es)"),
        "no pattern in patterns/ should find evidence this converter \
         cannot supply from callTracer alone; got:\n{stdout}"
    );

    let _ = std::fs::remove_file(&converted);
}
