use std::collections::HashMap;

use super::*;

fn provider(id: &str, kind: AsrProviderKind) -> AsrProviderConfig {
    let endpoint =
        (kind == AsrProviderKind::Remote).then(|| "https://example.invalid/asr".to_owned());
    let command = (kind == AsrProviderKind::Command).then(|| "/bin/true".to_owned());
    AsrProviderConfig {
        id: id.to_owned(),
        kind,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command,
        args: Vec::new(),
        env: HashMap::new(),
        endpoint,
    }
}

fn configured_value(config: &VinpstConfig, provider_id: &str) -> Option<String> {
    config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.hotwords_file.clone())
}

#[test]
fn provider_options_include_only_hotword_capable_backends() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.asr.providers = vec![
        provider("local", AsrProviderKind::Local),
        provider("remote", AsrProviderKind::Remote),
        provider("command", AsrProviderKind::Command),
    ];
    config.asr.active_provider = "remote".to_owned();

    let options = hotword_provider_options(&config);
    assert_eq!(
        options
            .iter()
            .map(HotwordProviderSelection::id)
            .collect::<Vec<_>>(),
        vec!["local", "command"]
    );
    assert_eq!(
        HotwordEditorState::from_config(&config, None)
            .selected_provider
            .as_deref(),
        Some("local")
    );
}

#[test]
fn path_mutation_sets_clears_and_rejects_remote_providers() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.asr.providers = vec![
        provider("local", AsrProviderKind::Local),
        provider("remote", AsrProviderKind::Remote),
        provider("command", AsrProviderKind::Command),
    ];
    config.asr.active_provider = "local".to_owned();

    let updated =
        update_hotword_path(&config, "local", Some("  words.txt  ")).expect("set hotword path");
    assert_eq!(
        updated.asr.providers[0].hotwords_file.as_deref(),
        Some("words.txt")
    );
    let cleared = update_hotword_path(&updated, "local", None).expect("clear hotword path");
    assert_eq!(cleared.asr.providers[0].hotwords_file, None);
    assert!(update_hotword_path(&config, "remote", Some("words.txt")).is_err());
    assert!(update_hotword_path(&config, "command", Some("https://example/hotwords")).is_err());
}

#[test]
fn configured_path_whitespace_is_normalized_before_dirty_comparison() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let mut local = provider("local", AsrProviderKind::Local);
    local.hotwords_file = Some("  words.txt  ".to_owned());
    config.asr.providers = vec![local];
    config.asr.active_provider = "local".to_owned();

    let editor = HotwordEditorState::from_config(&config, Some("local"));
    assert_eq!(editor.configured_path, Some(PathBuf::from("words.txt")));
    assert_eq!(editor.path_input, "words.txt");
    assert!(!editor.path_is_dirty());
}

#[test]
fn content_path_refuses_cross_process_relative_ambiguity() {
    let directory = tempfile::tempdir().expect("temp dir");
    let model_directory = directory.path().join("paraformer");
    fs::create_dir(&model_directory).expect("model directory");
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let mut local = provider("local", AsrProviderKind::Local);
    local.model = Some(model_directory.to_string_lossy().into_owned());
    local.hotwords_file = Some("hotwords.txt".to_owned());
    let mut command = provider("command", AsrProviderKind::Command);
    command.hotwords_file = Some("relative-command-hotwords.txt".to_owned());
    config.asr.providers = vec![local, command];
    config.asr.active_provider = "local".to_owned();

    assert_eq!(
        resolved_hotword_content_path(&config, "local").expect("resolve local hotwords"),
        Some(model_directory.join("hotwords.txt"))
    );
    fs::remove_dir(&model_directory).expect("remove model directory");
    assert!(resolved_hotword_content_path(&config, "local").is_err());
    fs::create_dir(&model_directory).expect("restore model directory");
    config.asr.providers[0].model = Some("paraformer".to_owned());
    let local_error = resolved_hotword_content_path(&config, "local")
        .expect_err("relative local model and hotword are ambiguous");
    assert!(local_error.contains("daemon process environment"));

    config.asr.providers[0].hotwords_file = Some("/tmp/local-hotwords.txt".to_owned());
    assert_eq!(
        resolved_hotword_content_path(&config, "local").expect("absolute local hotwords"),
        Some(PathBuf::from("/tmp/local-hotwords.txt"))
    );
    config.asr.providers[0].hotwords_file = Some("https://example.invalid/hotwords.txt".to_owned());
    let url_error = resolved_hotword_content_path(&config, "local")
        .expect_err("URL-like local hotwords are not filesystem paths");
    assert!(url_error.contains("URL-like"));
    config.asr.providers[0].hotwords_file = Some("/tmp/local-hotwords.txt".to_owned());

    let command_error = resolved_hotword_content_path(&config, "command")
        .expect_err("relative command path is external");
    assert!(command_error.contains("external command"));

    config.asr.providers[1].hotwords_file = Some("/tmp/command-hotwords.txt".to_owned());
    assert_eq!(
        resolved_hotword_content_path(&config, "command")
            .expect("resolve absolute command hotwords"),
        Some(PathBuf::from("/tmp/command-hotwords.txt"))
    );
}

#[test]
fn content_save_rejects_external_config_target_changes_before_write() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = directory.path().join("config.json");
    let old_path = directory.path().join("old-hotwords.txt");
    let new_path = directory.path().join("new-hotwords.txt");
    fs::write(&old_path, "alpha\n").expect("old hotwords fixture");

    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let mut local = provider("local", AsrProviderKind::Local);
    local.model = Some(
        directory
            .path()
            .join("model")
            .to_string_lossy()
            .into_owned(),
    );
    local.hotwords_file = Some(old_path.to_string_lossy().into_owned());
    config.asr.providers = vec![local];
    config.asr.active_provider = "local".to_owned();
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");
    let document = ConfigDocument {
        path: config_path.clone(),
        from_disk: true,
        config: config.clone(),
    };
    let baseline = read_hotword_snapshot(&old_path).expect("read hotwords");

    config.asr.providers[0].hotwords_file = Some(new_path.to_string_lossy().into_owned());
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize external config"),
    )
    .expect("write external config");

    let error = save_hotword_content_for_document(
        &document,
        "local",
        &old_path,
        &baseline,
        "should-not-write\n",
    )
    .expect_err("reject external config change");
    assert!(error.contains("changed on disk"));
    assert_eq!(
        fs::read_to_string(&old_path).expect("old content"),
        "alpha\n"
    );
    assert!(!new_path.exists());
}

#[test]
fn provider_selection_hides_and_restores_its_pending_activation() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config
        .asr
        .providers
        .push(provider("command", AsrProviderKind::Command));
    let active_provider = config.asr.active_provider.clone();
    let mut editor = HotwordEditorState::from_config(&config, Some(&active_provider));
    editor.pending_activation = Some(PendingHotwordActivation::for_config(
        active_provider.clone(),
        configured_value(&config, &active_provider),
    ));
    assert!(editor.pending_activation_for_selected_provider());

    editor.select_provider(&config, "command");
    assert!(editor.pending_activation.is_some());
    assert!(!editor.pending_activation_for_selected_provider());

    editor.select_provider(&config, &active_provider);
    assert!(editor.pending_activation_for_selected_provider());
}

#[test]
fn config_refresh_preserves_only_current_pending_activation() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = directory.path().join("config.json");
    let hotword_path = directory.path().join("hotwords.txt");
    fs::write(&hotword_path, "alpha\n").expect("hotword fixture");

    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.asr.providers[0].hotwords_file = Some(hotword_path.to_string_lossy().into_owned());
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("config fixture");
    let document = Ok(ConfigDocument {
        path: config_path,
        from_disk: true,
        config: config.clone(),
    });
    let mut app = crate::test_support::GuiHarness::new();
    app.refresh_hotword_editor(&document);
    app.hotword_editor.pending_activation = Some(PendingHotwordActivation::for_file(
        config.asr.active_provider.clone(),
        hotword_path.clone(),
        read_hotword_snapshot(&hotword_path).expect("pending baseline"),
    ));

    app.refresh_hotword_editor(&document);
    assert!(app.hotword_editor.pending_activation.is_some());

    fs::write(&hotword_path, "external\n").expect("external hotword update");
    app.refresh_hotword_editor(&document);
    assert!(app.hotword_editor.pending_activation.is_none());
}

#[test]
fn resetting_temporary_edits_preserves_pending_activation() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.asr.providers[0].hotwords_file = Some("/tmp/hotwords.txt".to_owned());
    let mut editor = HotwordEditorState::from_config(&config, None);
    editor.pending_activation = Some(PendingHotwordActivation::for_config(
        config.asr.active_provider.clone(),
        configured_value(&config, &config.asr.active_provider),
    ));
    editor.path_input = "/tmp/temporary-edit.txt".to_owned();
    assert!(editor.path_is_dirty());

    editor.reset_changes();
    assert!(!editor.path_is_dirty());
    assert!(editor.pending_activation.is_some());

    editor.loaded_path = editor.content_path.clone();
    editor.baseline = Some(HotwordContentSnapshot {
        existed: true,
        content: "alpha\n".to_owned(),
        version: None,
    });
    editor.content = text_editor::Content::with_text("temporary content\n");
    assert!(editor.content_is_dirty());
    editor.reset_changes();
    assert!(!editor.content_is_dirty());
    assert!(editor.pending_activation.is_some());
}

#[test]
fn unsafe_post_reload_snapshot_requires_a_fresh_load() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.asr.providers[0].hotwords_file = Some("/tmp/hotwords.txt".to_owned());
    let active_provider = config.asr.active_provider.clone();
    let mut editor = HotwordEditorState::from_config(&config, Some(&active_provider));
    editor.loaded_path.clone_from(&editor.content_path);
    editor.baseline = Some(HotwordContentSnapshot {
        existed: true,
        content: "loaded\n".to_owned(),
        version: None,
    });
    editor.content = text_editor::Content::with_text("gui-write\n");
    editor.pending_activation = Some(PendingHotwordActivation::for_config(
        active_provider.clone(),
        configured_value(&config, &active_provider),
    ));

    editor.apply_saved_content_baseline(&active_provider, None, false);

    assert!(editor.baseline.is_none());
    assert!(editor.loaded_path.is_none());
    assert_eq!(editor.content.text(), "gui-write\n");
    assert!(editor.pending_activation.is_none());
    assert!(!editor.content_is_dirty());
}

#[test]
fn hotword_messages_redact_paths_and_loaded_content() {
    let path_message = HotwordMessage::PathChanged(SecretInput::new(
        "/home/user/private/hotwords.txt".to_owned(),
    ));
    assert!(!format!("{path_message:?}").contains("/home/user"));

    let loaded = LoadedHotwordContent {
        provider_id: "local".to_owned(),
        path: PathBuf::from("/home/user/private/hotwords.txt"),
        snapshot: HotwordContentSnapshot {
            existed: true,
            content: "private phrase".to_owned(),
            version: None,
        },
    };
    let message = HotwordMessage::ContentLoaded {
        operation_id: 7,
        result: Ok(loaded),
    };
    let debug = format!("{message:?}");
    assert!(!debug.contains("private phrase"));
    assert!(!debug.contains("/home/user"));

    let retry_message = HotwordMessage::ActivationRetried {
        operation_id: 8,
        result: Err("config /home/user/private/config.json changed".to_owned()),
    };
    assert!(!format!("{retry_message:?}").contains("/home/user"));

    let load_error = HotwordMessage::ContentLoaded {
        operation_id: 9,
        result: Err("read /home/user/private/hotwords.txt failed".to_owned()),
    };
    let save_error = HotwordMessage::ContentSaved {
        operation_id: 10,
        result: Err("config /home/user/private/config.json changed".to_owned()),
    };
    let mutation_error = HotwordMessage::MutationFinished(Err(
        "save /home/user/private/config.json failed".to_owned(),
    ));
    for message in [
        format!("{load_error:?}"),
        format!("{save_error:?}"),
        format!("{mutation_error:?}"),
    ] {
        assert!(!message.contains("/home/user"));
    }
}

#[test]
fn file_picker_selection_updates_only_the_path_draft_and_clears_stale_content() {
    let mut app = crate::test_support::GuiHarness::new();
    let old_path = PathBuf::from("/tmp/old-hotwords.txt");
    app.hotword_editor.path_input = old_path.to_string_lossy().into_owned();
    app.hotword_editor.loaded_path = Some(old_path);
    app.hotword_editor.baseline = Some(HotwordContentSnapshot {
        existed: true,
        content: "old content
"
        .to_owned(),
        version: None,
    });
    app.hotword_editor.content = text_editor::Content::with_text(
        "old content
",
    );

    app.finish_hotword_file_browse(Ok(Some(SecretInput::new(
        "/tmp/new-hotwords.txt".to_owned(),
    ))));

    assert_eq!(app.hotword_editor.path_input, "/tmp/new-hotwords.txt");
    assert!(app.hotword_editor.path_is_dirty());
    assert!(app.hotword_editor.loaded_path.is_none());
    assert!(app.hotword_editor.baseline.is_none());
    assert!(app.hotword_editor.content.text().is_empty());
    assert!(matches!(app.operation, OperationState::Succeeded(_)));
}

#[test]
fn file_picker_cancel_preserves_the_current_draft() {
    let mut app = crate::test_support::GuiHarness::new();
    app.hotword_editor.path_input = "/tmp/pending-hotwords.txt".to_owned();
    app.operation = OperationState::Running("fixture");

    app.finish_hotword_file_browse(Ok(None));

    assert_eq!(app.hotword_editor.path_input, "/tmp/pending-hotwords.txt");
    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn dirty_content_blocks_file_picker_entry() {
    let mut app = crate::test_support::GuiHarness::new();
    app.hotword_editor.baseline = Some(HotwordContentSnapshot {
        existed: true,
        content: "baseline
"
        .to_owned(),
        version: None,
    });
    app.hotword_editor.content = text_editor::Content::with_text(
        "edited
",
    );

    drop(app.begin_hotword_file_browse());

    assert!(app.hotword_editor.content_is_dirty());
    assert!(matches!(app.operation, OperationState::Failed(_)));
}

#[test]
fn file_picker_messages_redact_selected_paths_and_errors() {
    let selected = HotwordMessage::PathPicked(Ok(Some(SecretInput::new(
        "/home/user/private/hotwords.txt".to_owned(),
    ))));
    let failed = HotwordMessage::PathPicked(Err(
        "portal failed for /home/user/private/hotwords.txt".to_owned(),
    ));

    assert!(!format!("{selected:?}").contains("/home/user"));
    assert!(!format!("{failed:?}").contains("/home/user"));
}
