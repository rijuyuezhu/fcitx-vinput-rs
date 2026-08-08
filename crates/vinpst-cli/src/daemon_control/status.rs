use super::{Context, MetadataExt, Path, PathBuf, dbus, fs};

pub(super) type DaemonAsrBackendStateTuple = (
    String,
    String,
    String,
    String,
    String,
    bool,
    bool,
    Vec<String>,
);

pub(super) fn print_daemon_status(dry_run: bool, json_output: bool) -> anyhow::Result<()> {
    if dry_run {
        let output = daemon_status_dry_run_json();
        if json_output {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            print_daemon_status_dry_run_text();
        }
        return Ok(());
    }

    let snapshot = daemon_status_via_dbus()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        print_daemon_status_text(&snapshot);
    }
    Ok(())
}

fn daemon_status_dry_run_json() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "will_call_dbus": false,
        "dbus": daemon_status_dbus_plan_json(),
        "reports": [
            "service_status",
            "bus_owner",
            "daemon_handoff",
            "asr_backend",
            "runtime_status",
            "text_adapters"
        ],
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinpst daemon status without --dry-run to query live daemon diagnostics",
            "run vinpst adapter status to inspect text adapter PID/running state",
            "run vinpst doctor to inspect local setup and activation readiness"
        ],
    })
}

fn daemon_status_dbus_plan_json() -> serde_json::Value {
    serde_json::json!({
        "service": dbus::SERVICE_BUS_NAME,
        "object_path": dbus::SERVICE_OBJECT_PATH,
        "interface": dbus::SERVICE_INTERFACE,
        "methods": [
            dbus::method::GET_STATUS,
            dbus::method::GET_ASR_BACKEND_STATE,
            dbus::method::GET_RUNTIME_STATUS,
        ],
    })
}

fn print_daemon_status_dry_run_text() {
    println!("Daemon status preview");
    println!("No daemon will be contacted.");
    println!();
    println!("Run `vinpst daemon status` to query the current daemon.");
}

pub(crate) fn daemon_owner_probe_plan_json() -> serde_json::Value {
    serde_json::json!({
        "service": "org.freedesktop.DBus",
        "object_path": "/org/freedesktop/DBus",
        "interface": "org.freedesktop.DBus",
        "target_name": dbus::SERVICE_BUS_NAME,
        "methods": [
            "GetNameOwner",
            "GetConnectionUnixProcessID"
        ],
        "process_fields": [
            "unix_process_id",
            "exe",
            "cmdline"
        ],
        "stale_owner_hints": [
            "runtime-status-unavailable",
            "unexpected owner executable",
            "activation service points to an old daemon path",
            "owner executable inode was deleted during package replacement"
        ]
    })
}

pub(super) fn daemon_status_via_dbus() -> anyhow::Result<serde_json::Value> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let status: String = proxy
        .call(dbus::method::GET_STATUS, &())
        .context("call GetStatus on daemon D-Bus service")?;
    // Creating a proxy does not necessarily activate a D-Bus service. Collect owner
    // diagnostics immediately after the first successful method call so an
    // activation-backed daemon is visible on the initial `daemon status` query.
    let owner = daemon_owner_diagnostics(&connection);
    let handoff = daemon_handoff_diagnostics(&owner);
    let asr: DaemonAsrBackendStateTuple = proxy
        .call(dbus::method::GET_ASR_BACKEND_STATE, &())
        .context("call GetAsrBackendState on daemon D-Bus service")?;
    let runtime_status_json: String = proxy
        .call(dbus::method::GET_RUNTIME_STATUS, &())
        .context("call GetRuntimeStatus on daemon D-Bus service")?;
    let runtime_status = serde_json::from_str::<serde_json::Value>(&runtime_status_json)
        .context("parse daemon runtime status JSON")?;
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "will_call_dbus": true,
        "dbus": daemon_status_dbus_plan_json(),
        "status": status,
        "asr_backend": {
            "target_provider_id": asr.0,
            "target_model_id": asr.1,
            "effective_provider_id": asr.2,
            "effective_model_id": asr.3,
            "last_error": asr.4,
            "reload_in_progress": asr.5,
            "has_effective_backend": asr.6,
            "remote_endpoints": asr.7,
        },
        "runtime_status": runtime_status,
        "owner": owner,
        "handoff": handoff,
    }))
}

fn daemon_owner_diagnostics(connection: &zbus::blocking::Connection) -> serde_json::Value {
    let mut output = serde_json::json!({
        "service": dbus::SERVICE_BUS_NAME,
        "unique_name": null,
        "unix_process_id": null,
        "process": null,
        "ok": false,
    });
    let bus_proxy = match zbus::blocking::Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ) {
        Ok(proxy) => proxy,
        Err(error) => {
            output["error"] = serde_json::json!(error.to_string());
            return output;
        }
    };
    let owner = match bus_proxy.call::<_, _, String>("GetNameOwner", &(dbus::SERVICE_BUS_NAME)) {
        Ok(owner) => owner,
        Err(error) => {
            output["error"] = serde_json::json!(error.to_string());
            return output;
        }
    };
    output["unique_name"] = serde_json::json!(owner);
    let Some(owner) = output["unique_name"].as_str() else {
        return output;
    };
    match bus_proxy.call::<_, _, u32>("GetConnectionUnixProcessID", &(owner)) {
        Ok(pid) => {
            output["unix_process_id"] = serde_json::json!(pid);
            output["process"] = daemon_owner_process_json(pid);
            output["ok"] = serde_json::json!(true);
        }
        Err(error) => {
            output["error"] = serde_json::json!(error.to_string());
        }
    }
    output
}

pub(super) fn daemon_owner_process_json(pid: u32) -> serde_json::Value {
    let proc_root = PathBuf::from("/proc").join(pid.to_string());
    let uid = fs::metadata(&proc_root).ok().map(|metadata| metadata.uid());
    let exe = fs::read_link(proc_root.join("exe"))
        .ok()
        .map(|path| path.display().to_string());
    let cmdline = fs::read(proc_root.join("cmdline"))
        .ok()
        .map(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect::<Vec<_>>()
        })
        .filter(|parts| !parts.is_empty());
    let cgroup = fs::read_to_string(proc_root.join("cgroup")).ok();
    let start_time_ticks = proc_start_time_ticks(&proc_root);
    serde_json::json!({
        "exe": exe,
        "cmdline": cmdline.unwrap_or_default(),
        "uid": uid,
        "cgroup": cgroup,
        "start_time_ticks": start_time_ticks,
    })
}

fn proc_start_time_ticks(proc_root: &Path) -> Option<u64> {
    let stat = fs::read_to_string(proc_root.join("stat")).ok()?;
    let (_, fields_after_name) = stat.rsplit_once(") ")?;
    fields_after_name
        .split_whitespace()
        .nth(19)
        .and_then(|value| value.parse().ok())
}

pub(super) const DELETED_EXECUTABLE_SUFFIX: &str = " (deleted)";

fn daemon_handoff_diagnostics(owner: &serde_json::Value) -> serde_json::Value {
    let expected = expected_sibling_daemon_path().filter(|path| path.exists());
    daemon_handoff_diagnostics_for_paths(owner["process"]["exe"].as_str(), expected.as_deref())
}

pub(super) fn expected_sibling_daemon_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|parent| parent.join("vinpst-daemon"))
}

pub(super) fn daemon_handoff_diagnostics_for_paths(
    owner_executable: Option<&str>,
    expected_executable: Option<&Path>,
) -> serde_json::Value {
    let owner_executable_deleted =
        owner_executable.is_some_and(|path| path.ends_with(DELETED_EXECUTABLE_SUFFIX));
    let normalized_owner_executable =
        owner_executable.map(|path| path.strip_suffix(DELETED_EXECUTABLE_SUFFIX).unwrap_or(path));
    let path_matches = normalized_owner_executable
        .zip(expected_executable)
        .map(|(owner, expected)| executable_paths_match(Path::new(owner), expected));
    let reason = if owner_executable_deleted {
        Some("owner-executable-deleted")
    } else if path_matches == Some(false) {
        Some("owner-executable-path-mismatch")
    } else {
        None
    };
    let restart_recommended = reason.is_some();
    serde_json::json!({
        "expected_executable": expected_executable,
        "owner_executable": owner_executable,
        "normalized_owner_executable": normalized_owner_executable,
        "owner_executable_deleted": owner_executable_deleted,
        "path_matches": path_matches,
        "restart_recommended": restart_recommended,
        "reason": reason,
        "automatic_restart_performed": false,
        "next_step": restart_recommended.then_some("run vinpst daemon handoff"),
    })
}

fn executable_paths_match(owner: &Path, expected: &Path) -> bool {
    match (fs::canonicalize(owner), fs::canonicalize(expected)) {
        (Ok(owner), Ok(expected)) => owner == expected,
        _ => owner == expected,
    }
}

fn print_daemon_status_text(snapshot: &serde_json::Value) {
    println!("Daemon: {}", optional_json_str(&snapshot["status"]));
    if let Some(pid) = snapshot["owner"]["unix_process_id"].as_u64() {
        println!("Process: {pid}");
    }

    let provider = optional_json_str(&snapshot["asr_backend"]["effective_provider_id"]);
    let model = optional_json_str(&snapshot["asr_backend"]["effective_model_id"]);
    let backend_ready = snapshot["asr_backend"]["has_effective_backend"]
        .as_bool()
        .unwrap_or(false);
    println!(
        "ASR: {provider} ({})",
        if backend_ready {
            "ready"
        } else {
            "unavailable"
        }
    );
    if model != "-" {
        println!("Model: {model}");
    }
    if let Some(error) = snapshot["asr_backend"]["last_error"]
        .as_str()
        .filter(|error| !error.is_empty())
    {
        println!("ASR error: {error}");
    }

    let active_session = snapshot["runtime_status"]["active_session"]
        .as_bool()
        .unwrap_or(false);
    let adapter_count = snapshot["runtime_status"]["text_adapters"]["adapter_count"]
        .as_u64()
        .unwrap_or(0);
    println!(
        "Session: {}",
        if active_session { "active" } else { "idle" }
    );
    println!("Text adapters: {adapter_count}");

    if snapshot["handoff"]["restart_recommended"].as_bool() == Some(true) {
        println!();
        println!("The running daemon belongs to an older installation.");
        println!("Run `vinpst daemon handoff` to switch to the current daemon safely.");
    }
}

pub(crate) fn optional_json_str(value: &serde_json::Value) -> &str {
    value.as_str().unwrap_or("-")
}

pub(crate) fn daemon_service_proxy(
    connection: &zbus::blocking::Connection,
) -> anyhow::Result<zbus::blocking::Proxy<'_>> {
    zbus::blocking::Proxy::new(
        connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .context("create daemon D-Bus proxy")
}
