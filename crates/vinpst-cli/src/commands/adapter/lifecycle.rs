use super::{
    Context, InstalledAdapterResolution, LiveScriptKind, Path, PathBuf, TextAdapterState,
    daemon_owner_probe_plan_json, daemon_service_proxy, dbus,
};
use super::{
    catalog::{load_adapter_list_context, load_live_adapter_registry},
    mutation::normalize_adapter_id,
};

fn resolve_installed_adapter_selector(
    selector: &str,
    registry_path: Option<&Path>,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<InstalledAdapterResolution> {
    let selector = normalize_adapter_id(selector)?;
    let context = load_adapter_list_context(config_path)?;
    let (adapter_id, registry_source) = if context
        .config
        .llm
        .adapters
        .iter()
        .any(|adapter| adapter.id == selector)
    {
        (selector.clone(), None)
    } else if let Some(registry_path) = registry_path {
        let registry = load_live_adapter_registry(Some(registry_path), &context.config.registry)?;
        let entry = registry
            .registry
            .entry_by_id_or_short_id(&selector, LiveScriptKind::LlmAdapter)
            .with_context(|| format!("text adapter selector `{selector}` not found"))?;
        if !context
            .config
            .llm
            .adapters
            .iter()
            .any(|adapter| adapter.id == entry.id)
        {
            anyhow::bail!(
                "text adapter `{}` resolved from `{selector}` is not installed",
                entry.id
            );
        }
        (entry.id.clone(), Some(registry.source_json))
    } else {
        anyhow::bail!(
            "text adapter `{selector}` not found; pass --registry <adapters.json> to resolve a short id"
        );
    };
    Ok(InstalledAdapterResolution {
        selector,
        adapter_id,
        config_path: context.config_path,
        config_source: context.source,
        registry_source,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn print_adapter_lifecycle(
    action: &str,
    adapter_selector: &str,
    method: &str,
    registry_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let resolution =
        resolve_installed_adapter_selector(adapter_selector, registry_path, config_path)?;
    if !dry_run {
        call_adapter_lifecycle_via_dbus(method, &resolution.adapter_id)?;
    }
    let output = adapter_lifecycle_output(action, &resolution, method, dry_run);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if dry_run {
        println!("Would {action} text adapter `{}`.", resolution.adapter_id);
        println!("No daemon will be contacted.");
    } else {
        match action {
            "start" => println!("Text adapter `{}` started.", resolution.adapter_id),
            "stop" => println!("Text adapter `{}` stopped.", resolution.adapter_id),
            _ => println!(
                "Text adapter `{}`: {action} completed.",
                resolution.adapter_id
            ),
        }
    }
    Ok(())
}

fn adapter_lifecycle_output(
    action: &str,
    resolution: &InstalledAdapterResolution,
    method: &str,
    dry_run: bool,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "action": action,
        "selector": resolution.selector,
        "adapter_id": resolution.adapter_id,
        "config_path": resolution.config_path,
        "config_source": resolution.config_source,
        "registry_source": resolution.registry_source,
        "will_call_dbus": !dry_run,
        "called": !dry_run,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": method,
        },
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
            "run vinpst adapter list to verify configured text adapters",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn call_adapter_lifecycle_via_dbus(method: &str, adapter_id: &str) -> anyhow::Result<()> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let _: () = proxy
        .call(method, &(adapter_id))
        .with_context(|| format!("call {method} on daemon D-Bus service"))?;
    Ok(())
}

pub(super) fn print_adapter_status(
    adapter_selector: Option<&str>,
    registry_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let resolution = adapter_selector
        .map(|selector| resolve_installed_adapter_selector(selector, registry_path, config_path))
        .transpose()?;
    let output = if dry_run {
        adapter_status_plan_json(resolution.as_ref())
    } else {
        let state = call_text_adapter_state_via_dbus()?;
        adapter_status_state_json(resolution.as_ref(), &state)?
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_adapter_status_text(&output);
    }
    Ok(())
}

fn call_text_adapter_state_via_dbus() -> anyhow::Result<TextAdapterState> {
    let connection = zbus::blocking::Connection::session().context("connect to session bus")?;
    let proxy = daemon_service_proxy(&connection)?;
    let raw: String = proxy
        .call(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .context("call GetTextAdapterState on daemon D-Bus service")?;
    serde_json::from_str::<TextAdapterState>(&raw).context("parse GetTextAdapterState response")
}

fn adapter_status_plan_json(resolution: Option<&InstalledAdapterResolution>) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": true,
        "action": "status",
        "selector": resolution.map(|resolution| resolution.selector.as_str()),
        "adapter_id": resolution.map(|resolution| resolution.adapter_id.as_str()),
        "config_path": resolution.and_then(|resolution| resolution.config_path.as_ref()),
        "config_source": resolution.map(|resolution| resolution.config_source),
        "registry_source": resolution.and_then(|resolution| resolution.registry_source.as_ref()),
        "will_call_dbus": false,
        "called": false,
        "dbus": {
            "service": dbus::SERVICE_BUS_NAME,
            "object_path": dbus::SERVICE_OBJECT_PATH,
            "interface": dbus::SERVICE_INTERFACE,
            "method": dbus::method::GET_TEXT_ADAPTER_STATE,
        },
        "owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": [
            "run vinpst adapter status without --dry-run to query daemon runtime state",
            "run vinpst adapter start or stop to change adapter runtime state"
        ],
    })
}

fn adapter_status_state_json(
    resolution: Option<&InstalledAdapterResolution>,
    state: &TextAdapterState,
) -> anyhow::Result<serde_json::Value> {
    let state_json = serde_json::json!({
        "adapter_count": state.adapter_count,
        "adapter_ids": state.adapter_ids,
        "single_adapter_id": state.single_adapter_id,
    });
    if let Some(resolution) = resolution {
        let adapter = state
            .adapters
            .iter()
            .find(|adapter| adapter.id == resolution.adapter_id)
            .with_context(|| {
                format!(
                    "text adapter `{}` not found in daemon state",
                    resolution.adapter_id
                )
            })?;
        return Ok(serde_json::json!({
            "ok": true,
            "dry_run": false,
            "action": "status",
            "selector": resolution.selector,
            "adapter_id": resolution.adapter_id,
            "config_path": resolution.config_path,
            "config_source": resolution.config_source,
            "registry_source": resolution.registry_source,
            "state": state_json,
            "adapter": adapter,
        }));
    }
    Ok(serde_json::json!({
        "ok": true,
        "dry_run": false,
        "action": "status",
        "selector": serde_json::Value::Null,
        "adapter_id": serde_json::Value::Null,
        "config_path": serde_json::Value::Null,
        "config_source": serde_json::Value::Null,
        "registry_source": serde_json::Value::Null,
        "state": state_json,
        "adapters": state.adapters,
    }))
}

fn print_adapter_status_text(output: &serde_json::Value) {
    if output["dry_run"].as_bool().unwrap_or(false) {
        if let Some(adapter_id) = output["adapter_id"].as_str() {
            println!("Would query text adapter `{adapter_id}`.");
        } else {
            println!("Would query configured text adapters.");
        }
        println!("No daemon will be contacted.");
        return;
    }

    if let Some(adapter) = output.get("adapter") {
        print_adapter_status_row(adapter);
        return;
    }

    let count = output["state"]["adapter_count"].as_u64().unwrap_or(0);
    println!("Text adapters: {count}");
    if count == 0 {
        return;
    }
    println!("ID\tKIND\tSTATE\tPID");
    if let Some(adapters) = output["adapters"].as_array() {
        for adapter in adapters {
            print_adapter_status_row(adapter);
        }
    }
}

fn print_adapter_status_row(adapter: &serde_json::Value) {
    let running = adapter["is_running"].as_bool().unwrap_or(false);
    let pid = adapter["pid"]
        .as_u64()
        .map_or_else(|| "-".to_owned(), |pid| pid.to_string());
    println!(
        "{}\t{}\t{}\t{}",
        adapter["id"].as_str().unwrap_or("-"),
        adapter["kind"].as_str().unwrap_or("-"),
        if running { "running" } else { "stopped" },
        pid,
    );
}
