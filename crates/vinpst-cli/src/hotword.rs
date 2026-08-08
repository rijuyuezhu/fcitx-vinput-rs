//! Hotword CLI commands and config mutation flow.

use std::{
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::Context;
use clap::Subcommand;
use vinpst_config::{AsrProviderConfig, VinpstConfig};

use crate::{
    asr_provider_kind_label, config_set_write_target, default_config_path, hotword_supported,
    load_config_json, normalize_provider_id, split_editor_argv, validate_config_json_value,
    write_config_set_document,
};

/// Hotword configuration inspection commands.
#[derive(Debug, Subcommand)]
pub(crate) enum HotwordCommand {
    /// Show the hotwords file configured for the active or selected ASR provider.
    Get {
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Set the hotwords file for the active or selected ASR provider.
    Set {
        /// Hotwords file path to write into provider config.
        path: String,
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Clear the hotwords file for the active or selected ASR provider.
    Clear {
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Open the configured hotwords file in an editor.
    #[command(alias = "e")]
    Edit {
        /// Optional ASR provider id. Defaults to the active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Editor executable to run. Defaults to `$VINPST_HOTWORD_EDITOR`, `$VINPST_CONFIG_EDITOR`, `$EDITOR`, then `$VISUAL`.
        #[arg(long)]
        editor: Option<String>,
        /// Print the edit plan without launching the editor.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}

pub(crate) fn handle_hotword_command(command: HotwordCommand) -> anyhow::Result<()> {
    match command {
        HotwordCommand::Get {
            provider,
            config,
            json,
        } => print_hotword_get(provider.as_deref(), config.as_ref(), json),
        HotwordCommand::Set {
            path,
            provider,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_hotword_mutation(HotwordMutationRequest {
            provider_id: provider.as_deref(),
            hotwords_file: Some(&path),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        HotwordCommand::Clear {
            provider,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_hotword_mutation(HotwordMutationRequest {
            provider_id: provider.as_deref(),
            hotwords_file: None,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        HotwordCommand::Edit {
            provider,
            config,
            editor,
            dry_run,
            json,
        } => print_hotword_edit(HotwordEditRequest {
            provider_id: provider.as_deref(),
            config_path: config.as_ref(),
            editor: editor.as_deref(),
            dry_run,
            json_output: json,
        }),
    }
}

#[derive(Clone, Copy)]
struct HotwordEditRequest<'a> {
    provider_id: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    editor: Option<&'a str>,
    dry_run: bool,
    json_output: bool,
}

struct HotwordEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    active_provider: String,
    provider_id: String,
    provider_type: &'static str,
    hotwords_file: PathBuf,
    editor_argv: Vec<String>,
    dry_run: bool,
    edited: bool,
    exit_status: Option<i32>,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct HotwordMutationRequest<'a> {
    provider_id: Option<&'a str>,
    hotwords_file: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct HotwordMutationOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    active_provider: String,
    provider_id: String,
    provider_type: &'static str,
    before: Option<String>,
    after: Option<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct HotwordGetContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    active_provider: String,
    provider: AsrProviderConfig,
}

fn print_hotword_get(
    provider_id: Option<&str>,
    config_path: Option<&PathBuf>,
    json_output: bool,
) -> anyhow::Result<()> {
    let context = load_hotword_get_context(provider_id, config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&hotword_get_json(&context))?
        );
    } else {
        print_hotword_get_text(&context);
    }
    Ok(())
}

fn load_hotword_get_context(
    provider_id: Option<&str>,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<HotwordGetContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for hotword get")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for hotword get")?;
    config
        .validate()
        .context("validate config for hotword get")?;
    let selected_provider_id = provider_id
        .map(normalize_provider_id)
        .transpose()?
        .unwrap_or_else(|| config.asr.active_provider.clone());
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == selected_provider_id)
        .with_context(|| format!("ASR provider `{selected_provider_id}` not found"))?
        .clone();
    Ok(HotwordGetContext {
        config_path: loaded.path,
        source: loaded.source,
        active_provider: config.asr.active_provider,
        provider,
    })
}

fn hotword_get_json(context: &HotwordGetContext) -> serde_json::Value {
    let hotwords_file = context.provider.hotwords_file.as_deref();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "active_provider": context.active_provider.as_str(),
        "provider_id": context.provider.id.as_str(),
        "provider_type": asr_provider_kind_label(&context.provider.kind),
        "active": context.provider.id == context.active_provider,
        "supported": hotword_supported(&context.provider.kind),
        "configured": hotwords_file.is_some_and(|value| !value.trim().is_empty()),
        "hotwords_file": hotwords_file,
        "next_steps": [
            "run vinpst provider list to inspect configured ASR providers",
            "run vinpst hotword set <path> once hotword mutation support is available",
            "run vinpst asr-state to inspect the selected provider runtime readiness"
        ],
    })
}

fn print_hotword_get_text(context: &HotwordGetContext) {
    if !hotword_supported(&context.provider.kind) {
        println!(
            "ASR provider `{}` does not support hotwords.",
            context.provider.id
        );
        return;
    }
    if let Some(path) = context
        .provider
        .hotwords_file
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        println!("{path}");
    } else {
        println!(
            "No hotwords file is configured for `{}`.",
            context.provider.id
        );
    }
}

fn print_hotword_edit(request: HotwordEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_hotword_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&hotword_edit_json(&outcome))?
        );
    } else {
        print_hotword_edit_text(&outcome);
    }
    Ok(())
}

fn run_hotword_edit(request: &HotwordEditRequest<'_>) -> anyhow::Result<HotwordEditOutcome> {
    let context = load_hotword_get_context(request.provider_id, request.config_path)?;
    if !hotword_supported(&context.provider.kind) {
        anyhow::bail!(
            "ASR provider `{}` does not support hotwords",
            context.provider.id
        );
    }
    let hotwords_file = context
        .provider
        .hotwords_file
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| "No hotwords file configured. Use 'hotword set <path>' first.")?;
    let editor_argv = resolve_hotword_editor(request.editor)?;
    let mut edited = false;
    let mut exit_status = None;
    if !request.dry_run {
        let status = run_hotword_editor(&editor_argv, Path::new(hotwords_file))?;
        if !status.success() {
            anyhow::bail!("hotword editor exited with status {status}");
        }
        exit_status = status.code();
        edited = true;
    }
    Ok(HotwordEditOutcome {
        config_path: context.config_path,
        source: context.source,
        active_provider: context.active_provider,
        provider_id: context.provider.id,
        provider_type: asr_provider_kind_label(&context.provider.kind),
        hotwords_file: PathBuf::from(hotwords_file),
        editor_argv,
        dry_run: request.dry_run,
        edited,
        exit_status,
    })
}

fn hotword_edit_json(outcome: &HotwordEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "active_provider": outcome.active_provider,
        "provider_id": outcome.provider_id,
        "provider_type": outcome.provider_type,
        "hotwords_file": outcome.hotwords_file,
        "editor": outcome.editor_argv.join(" "),
        "editor_argv": outcome.editor_argv,
        "edited": outcome.edited,
        "exit_status": outcome.exit_status,
        "next_steps": [
            "run vinpst hotword get to verify the configured hotwords file",
            "run vinpst asr-state to inspect the selected provider runtime readiness"
        ],
    })
}

fn print_hotword_edit_text(outcome: &HotwordEditOutcome) {
    if outcome.dry_run {
        println!(
            "Would open hotwords file: {}",
            outcome.hotwords_file.display()
        );
    } else {
        println!("Edited hotwords file: {}", outcome.hotwords_file.display());
    }
}

fn resolve_hotword_editor(editor: Option<&str>) -> anyhow::Result<Vec<String>> {
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VINPST_HOTWORD_EDITOR").ok())
        .or_else(|| std::env::var("VINPST_CONFIG_EDITOR").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .with_context(
            || "hotword edit requires --editor or $VINPST_HOTWORD_EDITOR/$VINPST_CONFIG_EDITOR/$EDITOR/$VISUAL",
        )?;
    let argv = split_editor_argv(&editor);
    if argv.is_empty() {
        anyhow::bail!("hotword editor command is empty");
    }
    Ok(argv)
}

fn run_hotword_editor(
    editor_argv: &[String],
    path: &Path,
) -> anyhow::Result<std::process::ExitStatus> {
    let (program, args) = editor_argv
        .split_first()
        .with_context(|| "hotword editor command is empty")?;
    ProcessCommand::new(program)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("run hotword editor `{}`", editor_argv.join(" ")))
}

fn print_hotword_mutation(request: HotwordMutationRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_hotword_mutation(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&hotword_mutation_json(&outcome))?
        );
    } else {
        print_hotword_mutation_text(&outcome);
    }
    Ok(())
}

fn run_hotword_mutation(
    request: &HotwordMutationRequest<'_>,
) -> anyhow::Result<HotwordMutationOutcome> {
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for hotword mutation")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for hotword mutation")?;
    let provider_id = request
        .provider_id
        .map(normalize_provider_id)
        .transpose()?
        .unwrap_or_else(|| config.asr.active_provider.clone());
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .with_context(|| format!("ASR provider `{provider_id}` not found"))?;
    let provider = &config.asr.providers[provider_index];
    if !hotword_supported(&provider.kind) {
        anyhow::bail!("ASR provider `{provider_id}` does not support hotwords");
    }
    let provider_type = asr_provider_kind_label(&provider.kind);
    let before = provider.hotwords_file.clone();
    let after = request
        .hotwords_file
        .map(normalize_hotwords_file)
        .transpose()?;

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    let provider_object = providers
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("ASR provider `{provider_id}` is not a JSON object"))?;
    if let Some(after) = &after {
        provider_object.insert(
            "hotwords_file".to_owned(),
            serde_json::Value::String(after.clone()),
        );
    } else {
        provider_object.remove("hotwords_file");
    }
    validate_config_json_value(&loaded.document, "validate updated hotword config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(HotwordMutationOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        active_provider: config.asr.active_provider,
        provider_id,
        provider_type,
        before,
        after,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn normalize_hotwords_file(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("hotwords file cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn hotword_mutation_json(outcome: &HotwordMutationOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "active_provider": outcome.active_provider,
        "provider_id": outcome.provider_id,
        "provider_type": outcome.provider_type,
        "before": outcome.before,
        "after": outcome.after,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst hotword get to verify the configured hotwords file",
            "run vinpst asr-state to inspect the selected provider runtime readiness",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_hotword_mutation_text(outcome: &HotwordMutationOutcome) {
    let (preview, applied) = if let Some(path) = outcome.after.as_deref() {
        (
            format!(
                "Would set hotwords for `{}` to `{path}`.",
                outcome.provider_id
            ),
            format!("Set hotwords for `{}` to `{path}`.", outcome.provider_id),
        )
    } else {
        (
            format!("Would clear hotwords for `{}`.", outcome.provider_id),
            format!("Cleared hotwords for `{}`.", outcome.provider_id),
        )
    };
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}
