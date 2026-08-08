//! Integration tests for first-run `vinpst init` behavior.

mod common;

use common::{assert_json_success, vinpst_command, workspace_file};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).expect("create unique temp dir");
    path
}

fn assert_valid_config(path: &std::path::Path) {
    let output = vinpst_command()
        .args(["config", "validate"])
        .arg(path)
        .arg("--summary-only")
        .output()
        .expect("validate initialized config");
    let value = assert_json_success(output, "initialized config validate");
    assert_eq!(value["ok"], true);
}

#[test]
fn init_dry_run_json_uses_xdg_defaults_without_writes() {
    let root = unique_temp_dir("vinpst-cli-init-dry-run");
    let config_home = root.join("config-home");
    let data_home = root.join("data-home");
    let cache_home = root.join("cache-home");

    let output = vinpst_command()
        .args(["init", "--dry-run", "--json"])
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", root.join("home"))
        .output()
        .expect("run vinpst init dry-run json");

    let value = assert_json_success(output, "init dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["config"]["will_write"], true);
    assert_eq!(value["config"]["wrote"], false);
    assert_eq!(
        value["config"]["path"],
        config_home
            .join("fcitx-vinpst/config.json")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["directories"]["model_root"]["will_create"], true);
    assert_eq!(value["directories"]["cache_root"]["will_create"], true);
    assert_eq!(
        value["activation_service"]["user_service_path"],
        data_home
            .join("dbus-1/services/org.fcitx.Vinpst.service")
            .to_string_lossy()
            .as_ref()
    );
    assert!(
        value["activation_service"]["command_argv"]
            .as_array()
            .expect("activation command argv array")
            .iter()
            .any(|arg| arg == "--configured-backends")
    );

    assert!(
        !config_home.exists(),
        "dry-run should not create config home"
    );
    assert!(!data_home.exists(), "dry-run should not create data home");
    assert!(!cache_home.exists(), "dry-run should not create cache home");
}

#[test]
fn init_writes_default_config_and_managed_dirs_idempotently() {
    let root = unique_temp_dir("vinpst-cli-init-write");
    let config_home = root.join("config-home");
    let data_home = root.join("data-home");
    let cache_home = root.join("cache-home");
    let config_path = config_home.join("fcitx-vinpst/config.json");
    let model_root = data_home.join("fcitx-vinpst/models");
    let cache_root = cache_home.join("fcitx-vinpst");

    let output = vinpst_command()
        .args(["init", "--json"])
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", root.join("home"))
        .output()
        .expect("run vinpst init json");

    let value = assert_json_success(output, "init write json");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["config"]["existed"], false);
    assert_eq!(value["config"]["wrote"], true);
    assert_eq!(value["directories"]["model_root"]["created"], true);
    assert_eq!(value["directories"]["cache_root"]["created"], true);
    assert_valid_config(&config_path);
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&config_path)
            .expect("stat initialized config")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(model_root.is_dir());
    assert!(cache_root.is_dir());

    let original = std::fs::read_to_string(&config_path).expect("read initialized config");
    let output = vinpst_command()
        .args(["init", "--json"])
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("HOME", root.join("home"))
        .output()
        .expect("rerun vinpst init json");

    let value = assert_json_success(output, "init idempotent json");
    assert_eq!(value["config"]["existed"], true);
    assert_eq!(value["config"]["wrote"], false);
    assert_eq!(value["directories"]["model_root"]["created"], false);
    assert_eq!(value["directories"]["cache_root"]["created"], false);
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read rerun config"),
        original
    );
}

#[test]
fn init_force_overwrites_existing_config() {
    let root = unique_temp_dir("vinpst-cli-init-force");
    let config_path = root.join("custom/config.json");
    let model_root = root.join("custom-models");
    let cache_root = root.join("custom-cache");
    std::fs::create_dir_all(config_path.parent().expect("config parent")).unwrap();
    std::fs::write(&config_path, "{\"broken\": true}\n").expect("write placeholder config");

    let output = vinpst_command()
        .args(["init", "--json"])
        .arg("--config")
        .arg(&config_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--cache-root")
        .arg(&cache_root)
        .env("XDG_DATA_HOME", root.join("data-home"))
        .env("XDG_CACHE_HOME", root.join("cache-home"))
        .env("HOME", root.join("home"))
        .output()
        .expect("run vinpst init without force");

    let value = assert_json_success(output, "init existing config json");
    assert_eq!(value["config"]["existed"], true);
    assert_eq!(value["config"]["wrote"], false);
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read preserved config"),
        "{\"broken\": true}\n"
    );

    let output = vinpst_command()
        .args(["init", "--force", "--json"])
        .arg("--config")
        .arg(&config_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--cache-root")
        .arg(&cache_root)
        .env("XDG_DATA_HOME", root.join("data-home"))
        .env("XDG_CACHE_HOME", root.join("cache-home"))
        .env("HOME", root.join("home"))
        .output()
        .expect("run vinpst init force");

    let value = assert_json_success(output, "init force json");
    assert_eq!(value["force"], true);
    assert_eq!(value["config"]["existed"], true);
    assert_eq!(value["config"]["wrote"], true);
    assert_valid_config(&config_path);
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&config_path)
            .expect("stat forced config")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read forced config"),
        std::fs::read_to_string(workspace_file("data/default-config.json"))
            .expect("read bundled default config")
    );
}
