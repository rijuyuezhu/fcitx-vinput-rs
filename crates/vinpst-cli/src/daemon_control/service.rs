use super::status::daemon_status_via_dbus;
use super::{daemon_owner_probe_plan_json, dbus, optional_json_str};

pub(super) use vinpst_daemon_control::UserServiceCommand;
use vinpst_daemon_control::{
    DAEMON_SERVICE_NAME, UserServiceAction, run_user_service_command, user_service_command,
};

use crate::sandbox;

pub(super) fn print_daemon_user_service_plan(
    action: &str,
    log_lines: Option<u16>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let command = daemon_user_service_command(action, log_lines)?;
    if dry_run {
        let output = daemon_user_service_dry_run_json(action, &command);
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_daemon_user_service_dry_run_text(action, &command);
        }
        return Ok(());
    }

    let output = run_daemon_user_service_command(action, &command);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_daemon_user_service_result_text(&output);
    }
    Ok(())
}

pub(super) fn daemon_user_service_command(
    action: &str,
    log_lines: Option<u16>,
) -> anyhow::Result<UserServiceCommand> {
    if log_lines == Some(0) {
        anyhow::bail!("daemon log --lines must be greater than 0");
    }

    let service_action = match action {
        "stop" => Some(UserServiceAction::Stop),
        "restart" => Some(UserServiceAction::Restart),
        "disable-now" => Some(UserServiceAction::DisableNow),
        "daemon-reload" => Some(UserServiceAction::DaemonReload),
        "main-pid" => Some(UserServiceAction::MainPid),
        "log" => None,
        _ => anyhow::bail!("unsupported daemon user service action `{action}`"),
    };
    if let Some(action) = service_action {
        return Ok(user_service_command(action));
    }

    let mut target_args = sandbox::daemon_log_args(DAEMON_SERVICE_NAME);
    if let Some(lines) = log_lines {
        target_args.extend(["-n".to_owned(), lines.to_string()]);
    }
    let target_program =
        std::env::var("VINPST_DAEMON_JOURNALCTL").unwrap_or_else(|_| "journalctl".to_owned());
    let (program, args) = sandbox::wrap_host_command(target_program, target_args);
    Ok(UserServiceCommand { program, args })
}

pub(super) fn daemon_user_service_dry_run_json(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": action,
        "will_mutate_user_service": false,
        "strategy": "systemd-user-service",
        "tool": daemon_user_service_tool_json(action, command),
        "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
        "host_wrapper": daemon_user_service_host_wrapper_json(command),
        "command": command.display(),
        "command_argv": command.argv(),
        "owner_probe": daemon_owner_probe_plan_json(),
        "fallback": daemon_user_service_fallback(),
        "fallback_steps": daemon_user_service_fallback_steps(),
        "next_steps": daemon_user_service_next_steps(action),
    })
}

fn daemon_user_service_tool_json(action: &str, command: &UserServiceCommand) -> serde_json::Value {
    let (name, env_override) = daemon_user_service_tool(action);
    serde_json::json!({
        "name": name,
        "program": command.target_program(),
        "env_override": env_override,
        "overridden": std::env::var_os(env_override).is_some(),
    })
}

fn daemon_user_service_host_wrapper_json(
    command: &UserServiceCommand,
) -> Option<serde_json::Value> {
    command
        .host_wrapper_program()
        .map(sandbox::host_wrapper_json)
}

fn daemon_user_service_tool(action: &str) -> (&'static str, &'static str) {
    match action {
        "log" => ("journalctl", "VINPST_DAEMON_JOURNALCTL"),
        _ => ("systemctl", "VINPST_DAEMON_SYSTEMCTL"),
    }
}

fn print_daemon_user_service_dry_run_text(action: &str, command: &UserServiceCommand) {
    match action {
        "log" => println!("Would read Vinpst daemon logs."),
        "restart" => println!("Would restart the Vinpst daemon."),
        "stop" => println!("Would stop the Vinpst daemon."),
        _ => println!("Would run daemon service action `{action}`."),
    }
    if command.is_host_wrapped() {
        println!("The action would run on the Flatpak host.");
    }
    println!("No changes were made.");
}

pub(super) fn run_daemon_user_service_command(
    action: &str,
    command: &UserServiceCommand,
) -> serde_json::Value {
    let will_mutate_user_service = matches!(action, "stop" | "restart" | "daemon-reload");
    let outcome = run_user_service_command(command);
    if let Some(error) = outcome.error {
        return serde_json::json!({
            "ok": false,
            "dry_run": false,
            "action": action,
            "will_mutate_user_service": will_mutate_user_service,
            "strategy": "systemd-user-service",
            "tool": daemon_user_service_tool_json(action, command),
            "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
            "host_wrapper": daemon_user_service_host_wrapper_json(command),
            "command": command.display(),
            "command_argv": command.argv(),
            "owner_probe": daemon_owner_probe_plan_json(),
            "exit_status": null,
            "stdout": "",
            "stderr": "",
            "error": error,
            "fallback": daemon_user_service_fallback(),
            "fallback_steps": daemon_user_service_fallback_steps(),
            "next_steps": daemon_user_service_next_steps(action),
        });
    }
    serde_json::json!({
        "ok": outcome.ok,
        "dry_run": false,
        "action": action,
        "will_mutate_user_service": will_mutate_user_service,
        "strategy": "systemd-user-service",
        "tool": daemon_user_service_tool_json(action, command),
        "sandbox": sandbox::sandbox_json(command.is_host_wrapped()),
        "host_wrapper": daemon_user_service_host_wrapper_json(command),
        "command": command.display(),
        "command_argv": command.argv(),
        "owner_probe": daemon_owner_probe_plan_json(),
        "exit_status": outcome.exit_status,
        "stdout": outcome.stdout,
        "stderr": outcome.stderr,
        "fallback": daemon_user_service_fallback(),
        "fallback_steps": daemon_user_service_fallback_steps(),
        "next_steps": daemon_user_service_next_steps(action),
    })
}

fn print_daemon_user_service_result_text(output: &serde_json::Value) {
    let action = optional_json_str(&output["action"]);
    if action == "log" {
        if let Some(stdout) = output["stdout"].as_str().filter(|value| !value.is_empty()) {
            print!("{stdout}");
            if !stdout.ends_with('\n') {
                println!();
            }
        }
        if output["ok"].as_bool() == Some(true) {
            return;
        }
    }

    if output["ok"].as_bool() == Some(true) {
        match action {
            "stop" => println!("Vinpst daemon stopped."),
            "restart" => println!("Vinpst daemon restarted."),
            _ => println!("Daemon service action `{action}` completed."),
        }
        return;
    }

    println!("Daemon service action `{action}` failed.");
    if let Some(stdout) = output["stdout"].as_str().filter(|value| !value.is_empty()) {
        print!("{stdout}");
        if !stdout.ends_with('\n') {
            println!();
        }
    }
    if let Some(stderr) = output["stderr"].as_str().filter(|value| !value.is_empty()) {
        print!("{stderr}");
        if !stderr.ends_with('\n') {
            println!();
        }
    }
    if let Some(error) = output["error"].as_str().filter(|value| !value.is_empty()) {
        println!("Error: {error}");
    }
    if let Some(next_step) = output["next_steps"]
        .as_array()
        .and_then(|steps| steps.first())
        .and_then(serde_json::Value::as_str)
    {
        println!("Next: {next_step}");
    }
}

fn daemon_user_service_next_steps(action: &str) -> Vec<&'static str> {
    match action {
        "log" => vec![
            "adjust --lines to inspect more or fewer journal entries",
            "run vinpst daemon status to inspect live D-Bus/runtime state",
        ],
        "restart" => vec![
            "run vinpst daemon status to verify the restarted daemon",
            "run vinpst daemon log --lines 100 if restart failed",
        ],
        _ => vec![
            "run vinpst daemon status to verify daemon availability",
            "run vinpst daemon log --lines 100 if service control failed",
        ],
    }
}

fn daemon_user_service_fallback_steps() -> Vec<&'static str> {
    vec![
        "run vinpst activation-service --user-status to inspect the per-user D-Bus activation service",
        "run vinpst daemon start --dry-run --json to inspect activation strategy",
        "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
    ]
}

const fn daemon_user_service_fallback() -> &'static str {
    "inspect the per-user D-Bus activation service and daemon process manually"
}

pub(super) fn print_daemon_start(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if dry_run {
        let output = serde_json::json!({
            "ok": true,
            "dry_run": true,
            "action": "start",
            "will_call_dbus": false,
            "activation": {
                "strategy": "dbus-service-activation",
                "trigger_method": dbus::method::GET_STATUS,
            },
            "dbus": {
                "service": dbus::SERVICE_BUS_NAME,
                "object_path": dbus::SERVICE_OBJECT_PATH,
                "interface": dbus::SERVICE_INTERFACE,
                "method": dbus::method::GET_STATUS,
            },
            "owner_probe": daemon_owner_probe_plan_json(),
            "next_steps": daemon_start_next_steps(),
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("Would activate the Vinpst daemon.");
            println!("No daemon will be contacted.");
        }
        return Ok(());
    }

    let status = daemon_status_via_dbus()?;
    let output = serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "start",
        "will_call_dbus": true,
        "called": true,
        "activation": {
            "strategy": "dbus-service-activation",
            "trigger_method": dbus::method::GET_STATUS,
        },
        "daemon": status,
        "next_steps": daemon_start_next_steps(),
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Vinpst daemon is running ({}).",
            optional_json_str(&output["daemon"]["status"])
        );
    }
    Ok(())
}

fn daemon_start_next_steps() -> Vec<&'static str> {
    vec![
        "run vinpst daemon status to inspect live D-Bus/runtime state",
        "run vinpst daemon log --lines 100 if activation failed",
    ]
}
