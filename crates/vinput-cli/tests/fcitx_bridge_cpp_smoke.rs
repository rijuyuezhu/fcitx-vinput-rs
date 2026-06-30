//! `CMake` smoke tests for the retained C++ Fcitx5 bridge core.

mod common;

use std::process::{Command, Output};

use common::workspace_file;

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cpp_bridge_cmake_project_configures_builds_and_tests() {
    let source_dir = workspace_file("cpp/fcitx5-addon");
    let build_dir = workspace_file("target/tmp/fcitx5-addon-cmake-smoke");
    let _ = std::fs::remove_dir_all(&build_dir);
    std::fs::create_dir_all(&build_dir).expect("create C++ bridge CMake build dir");

    let configure = Command::new("cmake")
        .arg("-S")
        .arg(&source_dir)
        .arg("-B")
        .arg(&build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Debug")
        .arg("-DVINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF")
        .output()
        .expect("run CMake configure for C++ bridge");
    assert_success(&configure, "C++ bridge CMake configure");

    let build = Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .arg("--parallel")
        .output()
        .expect("run CMake build for C++ bridge");
    assert_success(&build, "C++ bridge CMake build");

    let test = Command::new("ctest")
        .arg("--test-dir")
        .arg(&build_dir)
        .arg("--output-on-failure")
        .output()
        .expect("run CTest for C++ bridge");
    assert_success(&test, "C++ bridge CTest");
}
