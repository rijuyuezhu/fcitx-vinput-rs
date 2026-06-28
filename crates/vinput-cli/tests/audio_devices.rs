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
    assert_eq!(value["live"], cfg!(feature = "pipewire-backend"));
    let devices = value["devices"].as_array().unwrap();
    if !cfg!(feature = "pipewire-backend") {
        assert_eq!(devices.len(), 0);
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
