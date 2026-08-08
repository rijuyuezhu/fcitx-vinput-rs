use std::{collections::HashMap, fs};

use super::*;

#[test]
fn bundled_snapshot_is_redacted_and_has_legacy_pages() {
    let snapshot = headless_snapshot(Some(Path::new("/missing/config.json")), false)
        .expect("build offline GUI snapshot");
    assert_eq!(snapshot["application"], "vinpst-gui");
    assert_eq!(
        snapshot["pages"],
        json!(["Control", "Resources", "LLM", "Hotwords"])
    );
    assert_eq!(snapshot["daemon"]["skipped"], true);
    assert_eq!(
        snapshot["interaction"]["keyboard"]["tab_focus_traversal"],
        true
    );
    assert_eq!(
        snapshot["interaction"]["accessibility_tree"]["available"],
        false
    );
    assert_eq!(
        snapshot["interaction"]["assistive_technology"]["release_policy"],
        "unsupported-in-0.1.0"
    );
    assert_eq!(
        snapshot["interaction"]["assistive_technology"]["fallbacks"]["management_command"],
        "vinpst"
    );
    assert_eq!(
        snapshot["interaction"]["assistive_technology"]["fallbacks"]["fcitx_reload_command"],
        "fcitx5-remote --check -r"
    );
    assert!(!snapshot.to_string().contains("api_key"));
}

#[test]
fn resource_filter_matches_provider_and_scene_rows() {
    let config = VinpstConfig::bundled_default().expect("bundled config");
    assert!(
        filtered_asr_rows(&config, "sherpa")
            .iter()
            .any(|row| row.contains("sherpa-onnx"))
    );
    assert!(
        filtered_scene_rows(&config, "raw")
            .iter()
            .any(|row| row.contains("__raw__"))
    );
}

#[test]
fn adapter_rows_never_expose_commands_or_environment() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "safe-adapter".to_owned(),
        command: "helper --token super-secret".to_owned(),
        args: vec!["--api-key".to_owned(), "another-secret".to_owned()],
        env: HashMap::from([("TOKEN".to_owned(), "env-secret".to_owned())]),
        working_dir: None,
        extra: HashMap::new(),
    });

    let rows = llm_adapter_rows(&config).join("\n");
    assert_eq!(rows, "safe-adapter · command adapter");
    assert!(!rows.contains("secret"));
    assert!(!rows.contains("token"));
}

#[test]
fn disk_config_is_validated_before_display() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.global.default_language = "zh-CN".to_owned();
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");

    let loaded = load_config_document(Some(&path)).expect("load config");
    assert!(loaded.from_disk);
    assert_eq!(loaded.config.global.default_language, "zh-CN");
}

#[test]
fn config_draft_applies_every_editable_field() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let provider = config
        .asr
        .providers
        .last()
        .expect("bundled provider")
        .id
        .clone();
    let scene = config
        .scenes
        .definitions
        .last()
        .expect("bundled scene")
        .id
        .clone();
    let mut draft = ConfigDraft::from_config(&config);
    draft.default_language = "zh-CN".to_owned();
    draft.capture_device = "test-source".to_owned();
    draft.normalize_audio = false;
    draft.input_gain = 1.4;
    draft.duck_output_while_recording = true;
    draft.duck_output_volume = 0.4;
    draft.vad_enabled = false;
    draft.vad_threshold = 0.65;
    draft.active_provider.clone_from(&provider);
    draft.active_scene.clone_from(&scene);

    draft.apply_to(&mut config);

    config.validate().expect("validate edited config");
    assert_eq!(config.global.default_language, "zh-CN");
    assert_eq!(config.global.capture_device, "test-source");
    assert!(!config.asr.normalize_audio);
    assert!((config.asr.input_gain - 1.4).abs() < f32::EPSILON);
    assert!(config.global.duck_output_while_recording);
    assert!((config.global.duck_output_volume - 0.4).abs() < f32::EPSILON);
    assert!(!config.asr.vad.enabled);
    assert!((config.asr.vad.threshold - 0.65).abs() < f32::EPSILON);
    assert_eq!(config.asr.active_provider, provider);
    assert_eq!(config.scenes.active_scene, scene);
}

#[test]
fn duck_volume_changes_only_while_ducking_is_enabled() {
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let mut app = crate::test_support::GuiHarness::with_config(
        config,
        "/tmp/vinpst-gui-duck-volume.json",
        Page::Control,
    );
    let original = app.draft.as_ref().expect("config draft").duck_output_volume;

    app.send(Message::ConfigDraft(ConfigDraftMessage::DuckOutput(false)));
    app.send(Message::ConfigDraft(ConfigDraftMessage::DuckVolume(0.15)));
    assert!(
        (app.draft.as_ref().expect("config draft").duck_output_volume - original).abs()
            < f32::EPSILON
    );

    app.send(Message::ConfigDraft(ConfigDraftMessage::DuckOutput(true)));
    app.send(Message::ConfigDraft(ConfigDraftMessage::DuckVolume(0.15)));
    assert!(
        (app.draft.as_ref().expect("config draft").duck_output_volume - 0.15).abs() < f32::EPSILON
    );
}

#[test]
fn in_flight_config_mutation_freezes_navigation_and_edit_messages() {
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let mut app = crate::test_support::GuiHarness::with_config(
        config,
        "/tmp/vinpst-gui-in-flight-config.json",
        Page::Resources,
    );
    app.begin_add_scene();
    let editor_before = format!("{:?}", app.scene_editor);
    let language_before = app
        .draft
        .as_ref()
        .expect("config draft")
        .default_language
        .clone();
    app.operation = OperationState::Running("Saving scene…");

    app.send(Message::ConfigDraft(ConfigDraftMessage::DefaultLanguage(
        "zh-CN".to_owned(),
    )));
    app.send(Message::SelectPage(Page::Control));
    app.send(Message::ReloadConfig);
    app.send(Message::Scene(SceneMessage::EditorChanged {
        field: SceneEditorField::Label,
        value: "late editor change".to_owned(),
    }));

    assert_eq!(app.page, Page::Resources);
    assert_eq!(
        app.draft
            .as_ref()
            .expect("preserved config draft")
            .default_language,
        language_before
    );
    assert_eq!(format!("{:?}", app.scene_editor), editor_before);

    app.send(Message::Scene(SceneMessage::MutationFinished(Err(
        "fixture completion".to_owned(),
    ))));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error == "fixture completion"
    ));
}

#[test]
fn llm_provider_forms_and_mutations_reject_dirty_control_draft() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.llm.providers.push(vinpst_config::LlmProviderConfig {
        id: "existing".to_owned(),
        base_url: "https://example.invalid/v1".to_owned(),
        api_key: String::new(),
        model: None,
        extra_body: serde_json::json!({}),
        extra: HashMap::new(),
    });
    let mut app = crate::test_support::GuiHarness::with_config(
        config.clone(),
        "/tmp/vinpst-gui-dirty-llm-provider.json",
        Page::Llm,
    );
    let mut draft = ConfigDraft::from_config(&config);
    draft.default_language = "zh-CN".to_owned();
    app.draft = Some(draft);

    app.send(Message::LlmProvider(LlmProviderMessage::BeginAdd));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert!(app.llm_provider_editor.is_none());

    app.operation = OperationState::Idle;
    drop(
        app.update(Message::LlmProvider(LlmProviderMessage::BeginEdit(
            "existing".to_owned(),
        ))),
    );
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert!(app.llm_provider_editor.is_none());

    app.operation = OperationState::Idle;
    drop(app.update(Message::LlmProvider(LlmProviderMessage::Remove(
        "existing".to_owned(),
    ))));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert_eq!(
        app.config.as_ref().expect("config").config.llm.providers[0].id,
        "existing"
    );
    assert_eq!(
        app.draft
            .as_ref()
            .expect("preserved dirty draft")
            .default_language,
        "zh-CN"
    );
}

#[test]
fn scene_forms_reject_dirty_control_draft() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let scene_id = config.scenes.definitions[0].id.clone();
    app.config = Ok(ConfigDocument {
        path: PathBuf::from("/tmp/vinpst-gui-dirty-scene-form.json"),
        from_disk: false,
        config: config.clone(),
    });
    let mut draft = ConfigDraft::from_config(&config);
    draft.default_language = "zh-CN".to_owned();
    app.draft = Some(draft);
    app.page = Page::Resources;

    drop(app.update(Message::Scene(SceneMessage::BeginAdd)));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert!(app.scene_editor.is_none());

    app.operation = OperationState::Idle;
    drop(app.update(Message::Scene(SceneMessage::BeginEdit(scene_id))));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert!(app.scene_editor.is_none());
    assert_eq!(
        app.draft
            .as_ref()
            .expect("preserved dirty draft")
            .default_language,
        "zh-CN"
    );
}

#[test]
fn in_flight_llm_provider_mutation_freezes_form_messages() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = VinpstConfig::bundled_default().expect("bundled config");
    app.config = Ok(ConfigDocument {
        path: PathBuf::from("/tmp/vinpst-gui-in-flight-llm-provider.json"),
        from_disk: false,
        config: config.clone(),
    });
    app.draft = Some(ConfigDraft::from_config(&config));
    app.page = Page::Llm;
    drop(app.update(Message::LlmProvider(LlmProviderMessage::BeginAdd)));
    drop(
        app.update(Message::LlmProvider(LlmProviderMessage::EditorChanged {
            field: LlmProviderEditorField::Id,
            value: SecretInput::new("new-provider".to_owned()),
        })),
    );
    let editor_before = format!("{:?}", app.llm_provider_editor);
    let test_text_before = app.llm_provider_test_text.clone();
    app.operation = OperationState::Running("Saving LLM provider…");

    drop(
        app.update(Message::LlmProvider(LlmProviderMessage::EditorChanged {
            field: LlmProviderEditorField::Model,
            value: SecretInput::new("late-model".to_owned()),
        })),
    );
    drop(
        app.update(Message::LlmProvider(LlmProviderMessage::TestInputChanged(
            SecretInput::new("late test text".to_owned()),
        ))),
    );
    drop(app.update(Message::LlmProvider(LlmProviderMessage::CancelEdit)));
    drop(app.update(Message::SelectPage(Page::Control)));

    assert_eq!(app.page, Page::Llm);
    assert_eq!(format!("{:?}", app.llm_provider_editor), editor_before);
    assert_eq!(app.llm_provider_test_text, test_text_before);

    drop(
        app.update(Message::LlmProvider(LlmProviderMessage::MutationFinished(
            Err("fixture completion".to_owned()),
        ))),
    );
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error == "fixture completion"
    ));
}

#[test]
fn same_page_navigation_preserves_page_local_editors() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = VinpstConfig::bundled_default().expect("bundled config");
    app.config = Ok(ConfigDocument {
        path: PathBuf::from("/tmp/vinpst-gui-same-page-navigation.json"),
        from_disk: false,
        config: config.clone(),
    });
    app.draft = Some(ConfigDraft::from_config(&config));

    app.page = Page::Llm;
    drop(app.update(Message::LlmProvider(LlmProviderMessage::BeginAdd)));
    drop(
        app.update(Message::LlmProvider(LlmProviderMessage::EditorChanged {
            field: LlmProviderEditorField::Id,
            value: SecretInput::new("unsaved-provider".to_owned()),
        })),
    );
    let provider_editor_before = format!("{:?}", app.llm_provider_editor);
    drop(app.update(Message::SelectPage(Page::Llm)));
    assert_eq!(
        format!("{:?}", app.llm_provider_editor),
        provider_editor_before
    );
    assert!(provider_editor_before.contains("unsaved-provider"));

    drop(app.update(Message::SelectPage(Page::Resources)));
    assert!(app.llm_provider_editor.is_none());
    drop(app.update(Message::Scene(SceneMessage::BeginAdd)));
    drop(app.update(Message::Scene(SceneMessage::EditorChanged {
        field: SceneEditorField::Label,
        value: "Unsaved scene".to_owned(),
    })));
    let scene_editor_before = format!("{:?}", app.scene_editor);
    drop(app.update(Message::SelectPage(Page::Resources)));
    assert_eq!(format!("{:?}", app.scene_editor), scene_editor_before);
    assert!(scene_editor_before.contains("Unsaved scene"));

    drop(app.update(Message::SelectPage(Page::Control)));
    assert!(app.scene_editor.is_none());
}

#[test]
fn hotword_changes_block_navigation_and_reload_until_reset() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let document = Ok(ConfigDocument {
        path: PathBuf::from("/tmp/vinpst-gui-hotword-navigation.json"),
        from_disk: false,
        config: config.clone(),
    });
    app.refresh_hotword_editor(&document);
    app.config = document;
    app.draft = Some(ConfigDraft::from_config(&config));
    app.page = Page::Hotwords;

    drop(app.update(Message::Hotword(HotwordMessage::PathChanged(
        SecretInput::new("/tmp/unsaved-hotwords.txt".to_owned()),
    ))));
    drop(app.update(Message::SelectPage(Page::Control)));
    assert_eq!(app.page, Page::Hotwords);
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if !error.is_empty()
    ));

    drop(app.update(Message::ReloadConfig));
    assert_eq!(app.page, Page::Hotwords);
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if !error.is_empty()
    ));

    drop(app.update(Message::DismissError));
    assert!(matches!(app.operation, OperationState::Idle));
    drop(app.update(Message::Hotword(HotwordMessage::ResetChanges)));
    drop(app.update(Message::SelectPage(Page::Control)));
    assert_eq!(app.page, Page::Control);
}

#[test]
fn hotword_activation_retry_is_blocked_while_busy() {
    assert!(
        Message::Hotword(HotwordMessage::RetryActivation).blocked_while_busy(),
        "a queued retry must not start a second daemon reload"
    );
}

#[test]
fn failed_operation_uses_modal_state_without_inline_layout_notice() {
    let mut app = crate::test_support::GuiHarness::new();
    app.page = Page::Control;
    app.operation = OperationState::Failed("fixture failure".to_owned());

    assert!(app.operation_notice().is_none());
    assert!(app.error_dialog_view().is_some());
    assert!(app.is_busy());

    drop(app.update(Message::SelectPage(Page::Resources)));
    assert_eq!(
        app.page,
        Page::Control,
        "modal errors must block page changes"
    );
    drop(app.update(Message::FilterChanged("must-not-leak".to_owned())));
    assert!(
        app.filter.is_empty(),
        "modal errors must block focused page input"
    );

    drop(app.update(Message::Interaction(InteractionMessage::ClearFocus)));
    assert!(matches!(app.operation, OperationState::Idle));
    assert!(!app.has_error_dialog());
}

#[test]
fn resource_details_are_modal_and_escape_closes_them() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let provider_id = config.asr.active_provider.clone();
    app.config = Ok(ConfigDocument {
        path: PathBuf::from("/tmp/vinpst-gui-resource-detail-modal.json"),
        from_disk: false,
        config,
    });
    app.page = Page::Resources;
    app.select_asr_provider_detail(provider_id);

    assert!(app.has_resource_detail());
    assert!(app.resource_detail_view().is_some());

    drop(
        app.update(Message::Interaction(InteractionMessage::SelectPage(
            Page::Control,
        ))),
    );
    assert_eq!(app.page, Page::Resources);
    assert!(app.has_resource_detail());

    drop(app.update(Message::Interaction(InteractionMessage::ClearFocus)));
    assert!(!app.has_resource_detail());
    assert_eq!(app.page, Page::Resources);
}

#[test]
fn resource_mutations_reject_dirty_control_drafts_without_discarding_them() {
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let document = ConfigDocument {
        path: PathBuf::from("/tmp/vinpst-gui-dirty-draft.json"),
        from_disk: false,
        config: config.clone(),
    };
    let clean = ConfigDraft::from_config(&config);
    assert!(ensure_resource_mutation_draft_clean(&Ok(document.clone()), Some(&clean)).is_ok());

    let mut dirty = clean;
    dirty.default_language = "zh-CN".to_owned();
    let error = ensure_resource_mutation_draft_clean(&Ok(document), Some(&dirty))
        .expect_err("dirty Control draft must block resource mutation");
    assert!(error.contains("Save or reset"));
    assert_eq!(dirty.default_language, "zh-CN");
}

#[test]
fn provider_use_does_not_save_unrelated_dirty_audio_settings() {
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let original_provider = config.asr.active_provider.clone();
    let mut app = crate::test_support::GuiHarness::with_config(
        config,
        "/tmp/vinpst-gui-provider-use.json",
        Page::Control,
    );
    app.draft.as_mut().expect("config draft").capture_device = "unsaved-device".to_owned();

    app.send(Message::UseAsrProvider("other-provider".to_owned()));

    assert_eq!(
        app.draft.as_ref().expect("preserved draft").active_provider,
        original_provider
    );
    assert_eq!(
        app.draft.as_ref().expect("preserved draft").capture_device,
        "unsaved-device"
    );
    assert!(matches!(app.operation, OperationState::Failed(_)));
}

#[test]
fn config_draft_creates_missing_user_file_without_backup() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("nested/config.json");
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let document = ConfigDocument {
        path: path.clone(),
        from_disk: false,
        config,
    };
    let mut draft = ConfigDraft::from_config(&document.config);
    draft.default_language = "zh-CN".to_owned();

    let outcome = persist_config_draft(&document, &draft).expect("create user config");

    assert_eq!(outcome.path, path);
    assert_eq!(outcome.backup_path, None);
    assert_eq!(
        VinpstConfig::from_json_file(&outcome.path)
            .expect("load created config")
            .global
            .default_language,
        "zh-CN"
    );
}

#[test]
fn config_draft_replaces_existing_file_with_backup() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let config = VinpstConfig::bundled_default().expect("bundled config");
    write_config_file(&config, &path, None).expect("write original config");
    let document = load_config_document(Some(&path)).expect("load original config");
    let mut draft = ConfigDraft::from_config(&document.config);
    draft.capture_device = "replacement-source".to_owned();

    let outcome = persist_config_draft(&document, &draft).expect("replace user config");

    let backup_path = config_backup_path(&path);
    assert_eq!(outcome.backup_path.as_deref(), Some(backup_path.as_path()));
    assert_eq!(
        VinpstConfig::from_json_file(&path)
            .expect("load replaced config")
            .global
            .capture_device,
        "replacement-source"
    );
    assert_eq!(
        VinpstConfig::from_json_file(&backup_path)
            .expect("load backup config")
            .global
            .capture_device,
        config.global.capture_device
    );
}

#[test]
fn config_reload_failure_restore_reinstates_existing_document() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let config = VinpstConfig::bundled_default().expect("bundled config");
    write_config_file(&config, &path, None).expect("write original config");
    let document = load_config_document(Some(&path)).expect("load original config");
    let mut updated = config.clone();
    updated.global.capture_device = "new-source".to_owned();
    persist_updated_config(&document, &updated).expect("persist candidate config");

    restore_config_document(&document).expect("restore prior config");

    assert_eq!(
        VinpstConfig::from_json_file(&path).expect("restored config"),
        config
    );
}

#[test]
fn config_reload_failure_restore_removes_new_document() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let config = VinpstConfig::bundled_default().expect("bundled config");
    let document = ConfigDocument {
        path: path.clone(),
        from_disk: false,
        config: config.clone(),
    };
    let mut updated = config;
    updated.global.capture_device = "new-source".to_owned();
    persist_updated_config(&document, &updated).expect("persist candidate config");
    assert!(path.exists());

    restore_config_document(&document).expect("remove candidate config");

    assert!(!path.exists());
}

#[test]
fn config_draft_rejects_external_changes_without_overwrite() {
    let directory = tempfile::tempdir().expect("create temp dir");
    let path = directory.path().join("config.json");
    let config = VinpstConfig::bundled_default().expect("bundled config");
    write_config_file(&config, &path, None).expect("write original config");
    let document = load_config_document(Some(&path)).expect("load original config");
    let mut external = config.clone();
    external.global.capture_device = "external-source".to_owned();
    write_config_file(&external, &path, None).expect("write external update");
    let mut draft = ConfigDraft::from_config(&document.config);
    draft.capture_device = "gui-source".to_owned();

    let error =
        persist_config_draft(&document, &draft).expect_err("external update must block GUI save");

    assert!(error.contains("changed on disk"));
    assert_eq!(
        VinpstConfig::from_json_file(&path)
            .expect("load preserved external config")
            .global
            .capture_device,
        "external-source"
    );
    assert!(!config_backup_path(&path).exists());
}

#[test]
fn config_save_guard_requires_idle_daemon_without_active_session() {
    let idle = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": false}),
        text_adapters: TextAdapterState::default(),
    };
    assert!(ensure_config_save_allowed(&idle).is_ok());

    let recording = DaemonSnapshot {
        status: "recording".to_owned(),
        runtime: json!({"active_session": true}),
        text_adapters: TextAdapterState::default(),
    };
    assert!(ensure_config_save_allowed(&recording).is_err());

    let inconsistent = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": true}),
        text_adapters: TextAdapterState::default(),
    };
    assert!(ensure_config_save_allowed(&inconsistent).is_err());
}

#[test]
fn daemon_fallback_state_distinguishes_owner_loss_and_recovery() {
    let snapshot = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": false}),
        text_adapters: TextAdapterState::default(),
    };
    assert_eq!(
        daemon_state_from_poll(Ok(Some(snapshot.clone()))),
        DaemonLoadState::Ready(snapshot)
    );
    assert_eq!(daemon_state_from_poll(Ok(None)), DaemonLoadState::Stopped);
    assert_eq!(
        daemon_state_from_poll(Err("session bus unavailable".to_owned())),
        DaemonLoadState::Failed("session bus unavailable".to_owned())
    );
}

#[test]
fn daemon_owner_signals_reject_stale_snapshots_and_recover() {
    let mut app = crate::test_support::GuiHarness::new();
    assert_eq!(app.active_daemon_refresh_id, Some(1));

    let snapshot = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": false}),
        text_adapters: TextAdapterState::default(),
    };
    let task = app.update(Message::DaemonOwnerEvent(DaemonOwnerEvent::Connected {
        owned: true,
    }));
    assert_eq!(task.units(), 1);
    assert_eq!(app.daemon_owner_monitor, DaemonOwnerMonitorState::Ready);
    assert_eq!(app.active_daemon_refresh_id, Some(2));

    let _ = app.update(Message::DaemonOwnerEvent(DaemonOwnerEvent::Changed {
        owned: false,
    }));
    assert_eq!(app.active_daemon_refresh_id, None);
    assert_eq!(app.daemon, DaemonLoadState::Stopped);

    let _ = app.update(Message::DaemonLoaded {
        operation_id: 1,
        result: Ok(snapshot.clone()),
    });
    let _ = app.update(Message::DaemonLoaded {
        operation_id: 2,
        result: Ok(snapshot.clone()),
    });
    assert_eq!(app.daemon, DaemonLoadState::Stopped);

    let task = app.update(Message::DaemonOwnerEvent(DaemonOwnerEvent::Changed {
        owned: true,
    }));
    assert_eq!(task.units(), 1);
    assert_eq!(app.active_daemon_refresh_id, Some(3));
    let _ = app.update(Message::DaemonLoaded {
        operation_id: 3,
        result: Ok(snapshot.clone()),
    });
    assert_eq!(app.active_daemon_refresh_id, None);
    assert_eq!(app.daemon, DaemonLoadState::Ready(snapshot));
}

#[test]
fn daemon_monitor_failure_uses_serialized_non_activating_fallback() {
    let mut app = crate::test_support::GuiHarness::new();
    let snapshot = DaemonSnapshot {
        status: "idle".to_owned(),
        runtime: json!({"active_session": false}),
        text_adapters: TextAdapterState::default(),
    };
    let _ = app.update(Message::DaemonLoaded {
        operation_id: 1,
        result: Ok(snapshot.clone()),
    });

    let task = app.update(Message::DaemonOwnerEvent(DaemonOwnerEvent::Failed(
        "session bus signal stream failed".to_owned(),
    )));
    assert_eq!(task.units(), 1);
    assert!(matches!(
        app.daemon_owner_monitor,
        DaemonOwnerMonitorState::Failed(_)
    ));
    assert_eq!(app.daemon, DaemonLoadState::Ready(snapshot));
    assert_eq!(app.active_daemon_refresh_id, Some(2));

    let task = app.update(Message::DaemonFallbackPollTick);
    assert_eq!(task.units(), 0);
    let _ = app.update(Message::DaemonFallbackPolled {
        operation_id: 2,
        result: Ok(None),
    });
    assert_eq!(app.active_daemon_refresh_id, None);
    assert_eq!(app.daemon, DaemonLoadState::Stopped);
}

#[test]
fn model_install_cancel_completion_retains_exact_retry_selector() {
    let mut app = crate::test_support::GuiHarness::new();
    let first_task = app.update(Message::InstallRegistryModel("fixture-short-id".to_owned()));
    assert_eq!(first_task.units(), 1);
    assert!(app.model_install.is_active());

    let _ = app.update(Message::CancelModelInstall);
    let _ = app.update(Message::ModelInstalled {
        operation_id: 1,
        outcome: ModelInstallOutcome::Cancelled,
    });
    assert_eq!(
        app.model_install.retry_selector().as_deref(),
        Some("fixture-short-id")
    );

    let retry_task = app.update(Message::RetryModelInstall);
    assert_eq!(retry_task.units(), 1);
    assert!(app.model_install.is_active());
}
