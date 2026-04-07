//! Smoke test: every subcommand's `--help` exits 0 and prints something.
//!
//! No real DB or dataset interaction. Catches obvious clap structural
//! mistakes (missing field, wrong type, conflicting args).

use std::process::Command;

fn vbench() -> Command {
    let bin = env!("CARGO_BIN_EXE_vbench");
    Command::new(bin)
}

#[test]
fn root_help_runs() {
    let out = vbench()
        .arg("--help")
        .output()
        .expect("spawn vbench --help");
    assert!(out.status.success(), "vbench --help failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("vbench"));
    assert!(stdout.contains("list-datasets"));
    assert!(stdout.contains("list-adapters"));
    assert!(stdout.contains("fetch"));
    assert!(stdout.contains("run"));
    assert!(stdout.contains("inspect"));
    assert!(stdout.contains("cache"));
}

#[test]
fn version_runs() {
    let out = vbench()
        .arg("--version")
        .output()
        .expect("spawn vbench --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("vbench"));
}

#[test]
fn list_datasets_runs() {
    let out = vbench()
        .arg("list-datasets")
        .output()
        .expect("spawn vbench list-datasets");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cohere-1m"));
}

#[test]
fn list_adapters_runs() {
    let out = vbench()
        .arg("list-adapters")
        .output()
        .expect("spawn vbench list-adapters");
    assert!(out.status.success());
}

#[test]
fn run_help_runs() {
    let out = vbench()
        .args(["run", "--help"])
        .output()
        .expect("spawn vbench run --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--adapter"));
    assert!(stdout.contains("--dataset"));
    assert!(stdout.contains("--recall-k"));
    assert!(stdout.contains("--batch-size"));
}

#[test]
fn fetch_help_runs() {
    let out = vbench()
        .args(["fetch", "--help"])
        .output()
        .expect("spawn vbench fetch --help");
    assert!(out.status.success());
}

#[test]
fn cache_help_runs() {
    let out = vbench()
        .args(["cache", "--help"])
        .output()
        .expect("spawn vbench cache --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("show"));
    assert!(stdout.contains("clear"));
}

#[test]
fn inspect_help_runs() {
    let out = vbench()
        .args(["inspect", "--help"])
        .output()
        .expect("spawn vbench inspect --help");
    assert!(out.status.success());
}

#[test]
fn unknown_dataset_in_run_errors_clearly() {
    // No network access — should bail out before trying to download.
    let out = vbench()
        .args([
            "run",
            "--adapter",
            "strata",
            "--dataset",
            "definitely-not-a-real-dataset",
        ])
        .output()
        .expect("spawn vbench run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("definitely-not-a-real-dataset") || stderr.contains("unknown dataset"),
        "expected dataset error, got stderr: {stderr}"
    );
}
