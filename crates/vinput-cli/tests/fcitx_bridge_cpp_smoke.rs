//! Compile smoke tests for the retained C++ Fcitx5 bridge core.

mod common;

use std::process::Command;

use common::workspace_file;

#[test]
fn cpp_bridge_recognition_payload_core_compiles_and_runs() {
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "g++".to_owned());
    let out_dir = workspace_file("target/tmp");
    std::fs::create_dir_all(&out_dir).expect("create C++ bridge smoke output dir");
    let binary = out_dir.join("vinput-fcitx-bridge-smoke");

    let include_dir = workspace_file("cpp/fcitx5-addon/include");
    let source = workspace_file("cpp/fcitx5-addon/src/recognition_payload.cpp");
    let smoke = workspace_file("cpp/fcitx5-addon/tests/recognition_payload_smoke.cpp");

    let compile = Command::new(&compiler)
        .arg("-std=c++20")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-I")
        .arg(&include_dir)
        .arg(&source)
        .arg(&smoke)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap_or_else(|error| panic!("failed to start C++ compiler `{compiler}`: {error}"));

    assert!(
        compile.status.success(),
        "C++ bridge smoke compile failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        compile.status.code(),
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&binary)
        .output()
        .expect("run C++ bridge smoke binary");
    assert!(
        run.status.success(),
        "C++ bridge smoke binary failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
