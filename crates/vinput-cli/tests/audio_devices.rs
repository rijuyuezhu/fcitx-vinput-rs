//! Integration tests for audio device diagnostics CLI paths.

mod common;

use std::fs;

use common::{assert_json_success, vinput_command, write_temp_json};

#[test]
fn audio_devices_reports_default_capture_target_and_backend() {
    let output = vinput_command()
        .arg("audio-devices")
        .output()
        .expect("run vinput audio-devices");

    let value = assert_json_success(output, "audio devices summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["capture_device"], "default");
    assert_eq!(value["capture_target"]["kind"], "default");
    assert_eq!(
        value["capture_target"]["target_object"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["backend"],
        if cfg!(feature = "pipewire-backend") {
            "pipewire"
        } else {
            "unavailable"
        }
    );
    assert!(value["live"].is_boolean());
    let devices = value["devices"].as_array().unwrap();
    if value["live"] == true {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    } else {
        assert_eq!(devices.len(), 0);
    }
    if cfg!(feature = "pipewire-backend") {
        assert!(value["enumeration_error"].is_null() || value["enumeration_error"].is_string());
    } else {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    }
}

#[test]
fn audio_devices_preserves_configured_capture_target_object() {
    let path = write_temp_json(
        "vinput-audio-devices",
        r#"
        {
          "version": 1,
          "global": {"capture_device": "  alsa_input.usb-mic  "},
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = vinput_command()
        .args(["audio-devices", "--config"])
        .arg(&path)
        .output()
        .expect("run vinput audio-devices with config");
    fs::remove_file(&path).expect("remove temporary config fixture");

    let value = assert_json_success(output, "audio devices summary");
    assert_eq!(value["capture_device"], "  alsa_input.usb-mic  ");
    assert_eq!(value["capture_target"]["kind"], "object");
    assert_eq!(
        value["capture_target"]["target_object"],
        "alsa_input.usb-mic"
    );
}

#[cfg(feature = "pipewire-backend")]
#[test]
fn audio_devices_reports_pipewire_enumeration_error_without_failing() {
    let config_dir = std::env::temp_dir().join(format!(
        "vinput-missing-pipewire-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir(&config_dir).expect("create empty PipeWire config dir");

    let output = vinput_command()
        .env("PIPEWIRE_CONFIG_DIR", &config_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_DIRS", &config_dir)
        .arg("audio-devices")
        .output()
        .expect("run vinput audio-devices without PipeWire client config");
    fs::remove_dir(&config_dir).expect("remove empty PipeWire config dir");

    let value = assert_json_success(output, "audio devices summary without PipeWire config");
    assert_eq!(value["ok"], true);
    assert_eq!(value["backend"], "pipewire");
    assert_eq!(value["live"], false);
    assert_eq!(value["devices"].as_array().unwrap().len(), 0);
    assert!(
        value["enumeration_error"]
            .as_str()
            .is_some_and(|message| message.contains("enumerate PipeWire audio sources"))
    );
}
