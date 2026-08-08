use anyhow::Context;
use vinpst_protocol::{RecognitionPayload, ServiceStatus, dbus};

use crate::{
    RecordingCommand,
    daemon_control::{daemon_owner_probe_plan_json, daemon_service_proxy},
};

pub(crate) fn handle_recording_command(command: RecordingCommand) -> anyhow::Result<()> {
    match command {
        RecordingCommand::Start {
            selected_text,
            dry_run,
            json,
        } => print_recording_action("start", selected_text.as_deref(), None, dry_run, json),
        RecordingCommand::Stop {
            scene,
            dry_run,
            json,
        } => print_recording_action("stop", None, scene.as_deref(), dry_run, json),
        RecordingCommand::Status { dry_run, json } => print_recording_status(dry_run, json),
        RecordingCommand::Toggle {
            selected_text,
            scene,
            dry_run,
            json,
        } => print_recording_action(
            "toggle",
            selected_text.as_deref(),
            scene.as_deref(),
            dry_run,
            json,
        ),
    }
}

fn print_recording_status(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    let output = if dry_run {
        recording_status_plan_json()
    } else {
        recording_status_via_dbus()?
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_recording_status_text(&output);
    }
    Ok(())
}

fn recording_status_via_dbus() -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    Ok(recording_status_result_json(&status))
}

fn recording_status_plan_json() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": "status",
        "will_call_dbus": false,
        "called": false,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::GET_STATUS,
        },
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
            "run vinpst recording start --dry-run --json to inspect start calls",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn recording_status_result_json(status: &str) -> serde_json::Value {
    let parsed = ServiceStatus::parse_wire(status).ok();
    serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "status",
        "will_call_dbus": true,
        "called": true,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::GET_STATUS,
        },
        "status": status,
        "known_status": parsed.is_some(),
        "is_recording": parsed == Some(ServiceStatus::Recording),
        "is_busy": matches!(parsed, Some(ServiceStatus::Recording | ServiceStatus::Inferring | ServiceStatus::Postprocessing)),
    })
}

fn print_recording_status_text(output: &serde_json::Value) {
    if output["dry_run"].as_bool() == Some(true) {
        println!("Would query the current recording state.");
        println!("No daemon will be contacted.");
        return;
    }
    if let Some(status) = output["status"].as_str() {
        println!("Recording state: {status}");
    } else {
        println!("Recording state is unavailable.");
    }
}

fn print_recording_action(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    if dry_run {
        let output = recording_plan_json(action, selected_text, scene);
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_recording_plan_text(action, selected_text, scene);
        }
        return Ok(());
    }
    let result = recording_action_via_dbus(action, selected_text, scene)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_recording_result_text(&result);
    }
    Ok(())
}

fn recording_action_via_dbus(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let method = match (action, selected_text) {
        ("start", Some(text)) => {
            let _: () = proxy
                .call(dbus::method::START_COMMAND_RECORDING, &(text))
                .context("call StartCommandRecording on daemon D-Bus service")?;
            dbus::method::START_COMMAND_RECORDING
        }
        ("start", None) => {
            let _: () = proxy
                .call(dbus::method::START_RECORDING, &())
                .context("call StartRecording on daemon D-Bus service")?;
            dbus::method::START_RECORDING
        }
        ("stop", _) => {
            let payload: String = proxy
                .call(dbus::method::STOP_RECORDING, &(scene.unwrap_or("")))
                .context("call StopRecording on daemon D-Bus service")?;
            return Ok(recording_result_json(
                action,
                dbus::method::STOP_RECORDING,
                scene,
                Some(payload.as_str()),
            ));
        }
        ("toggle", _) => return recording_toggle_via_dbus(&proxy, selected_text, scene),
        _ => anyhow::bail!("unsupported recording action `{action}`"),
    };
    Ok(recording_result_json(action, method, scene, None))
}
fn recording_toggle_via_dbus(
    proxy: &zbus::blocking::Proxy<'_>,
    selected_text: Option<&str>,
    scene: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    if status == "recording" {
        let payload: String = proxy
            .call(dbus::method::STOP_RECORDING, &(scene.unwrap_or("")))
            .context("call StopRecording on daemon D-Bus service")?;
        let mut output = recording_result_json(
            "toggle",
            dbus::method::STOP_RECORDING,
            scene,
            Some(payload.as_str()),
        );
        output["status_before"] = serde_json::json!(status);
        return Ok(output);
    }
    let method = if let Some(text) = selected_text {
        let _: () = proxy
            .call(dbus::method::START_COMMAND_RECORDING, &(text))
            .context("call StartCommandRecording on daemon D-Bus service")?;
        dbus::method::START_COMMAND_RECORDING
    } else {
        let _: () = proxy
            .call(dbus::method::START_RECORDING, &())
            .context("call StartRecording on daemon D-Bus service")?;
        dbus::method::START_RECORDING
    };
    let mut output = recording_result_json("toggle", method, scene, None);
    output["status_before"] = serde_json::json!(status);
    Ok(output)
}

fn recording_result_json(
    action: &str,
    method: &str,
    scene: Option<&str>,
    payload_json: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": action,
        "will_call_dbus": true,
        "called": true,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": method,
        },
        "args": {
            "scene": scene.unwrap_or(""),
        },
        "payload_json": payload_json,
    })
}

fn print_recording_result_text(result: &serde_json::Value) {
    let action = result["action"].as_str().unwrap_or("recording");
    if let Some(payload_json) = result["payload_json"].as_str()
        && let Ok(payload) = RecognitionPayload::from_json_str(payload_json)
    {
        if !payload.commit_text.is_empty() {
            println!("{}", payload.commit_text);
            return;
        }
        if !payload.candidates.is_empty() {
            println!(
                "Recognition finished with {} candidates.",
                payload.candidates.len()
            );
            return;
        }
    }
    match action {
        "start" => println!("Recording started."),
        "stop" => println!("Recording stopped."),
        "toggle" => println!("Recording toggled."),
        _ => println!("Recording action completed."),
    }
}

fn recording_plan_json(
    action: &str,
    selected_text: Option<&str>,
    scene: Option<&str>,
) -> serde_json::Value {
    let methods = match (action, selected_text.is_some()) {
        ("start", true) => vec![dbus::method::START_COMMAND_RECORDING],
        ("start", false) => vec![dbus::method::START_RECORDING],
        ("stop", _) => vec![dbus::method::STOP_RECORDING],
        ("toggle", true) => vec![
            dbus::method::GET_STATUS,
            dbus::method::START_COMMAND_RECORDING,
            dbus::method::STOP_RECORDING,
        ],
        ("toggle", false) => vec![
            dbus::method::GET_STATUS,
            dbus::method::START_RECORDING,
            dbus::method::STOP_RECORDING,
        ],
        _ => Vec::new(),
    };
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": action,
        "will_call_dbus": false,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "methods": methods,
        },
        "owner_probe": daemon_owner_probe_plan_json(),
        "args": {
            "selected_text_present": selected_text.is_some(),
            "scene": scene.unwrap_or(""),
        },
        "next_steps": recording_action_next_steps(action),
    })
}

fn recording_action_next_steps(action: &str) -> Vec<&'static str> {
    match action {
        "stop" | "toggle" => vec![
            "run vinpst recording status --dry-run --json to inspect status calls",
            "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
        ],
        _ => vec![
            "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
            "run vinpst doctor to inspect full local diagnostics",
        ],
    }
}

fn print_recording_plan_text(action: &str, selected_text: Option<&str>, scene: Option<&str>) {
    match action {
        "start" if selected_text.is_some() => println!("Would start command-mode recording."),
        "start" => println!("Would start recording."),
        "stop" => println!("Would stop recording."),
        "toggle" => println!("Would toggle recording."),
        _ => println!("Would run recording action `{action}`."),
    }
    if let Some(scene) = scene.filter(|scene| !scene.is_empty()) {
        println!("Scene: {scene}");
    }
    println!("No daemon will be contacted.");
}
