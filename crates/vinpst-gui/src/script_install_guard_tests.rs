use vinpst_registry::LiveScriptKind;

use super::*;

#[test]
fn dirty_control_draft_blocks_script_install_and_removal_entry_points() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = vinpst_config::VinpstConfig::bundled_default().expect("bundled config");
    app.config = Ok(ConfigDocument {
        path: "/tmp/vinpst-gui-dirty-script-draft.json".into(),
        from_disk: false,
        config: config.clone(),
    });
    let mut draft = crate::ConfigDraft::from_config(&config);
    draft.default_language = "zh-CN".to_owned();
    app.draft = Some(draft);
    drop(app.begin_script_install(LiveScriptKind::AsrProvider, "fixture".to_owned()));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert!(matches!(app.script_install, ScriptInstallState::Idle));
    assert_eq!(
        app.draft
            .as_ref()
            .expect("preserved draft")
            .default_language,
        "zh-CN"
    );

    app.operation = OperationState::Idle;
    drop(app.begin_script_remove(
        LiveScriptKind::AsrProvider,
        "provider.fixture.batch".to_owned(),
    ));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("Save or reset")
    ));
    assert_eq!(
        app.draft
            .as_ref()
            .expect("preserved draft")
            .default_language,
        "zh-CN"
    );
}

#[test]
fn open_scene_editor_blocks_script_install_without_losing_input() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = vinpst_config::VinpstConfig::bundled_default().expect("bundled config");
    app.config = Ok(ConfigDocument {
        path: "/tmp/vinpst-gui-scene-script-draft.json".into(),
        from_disk: false,
        config: config.clone(),
    });
    app.draft = Some(crate::ConfigDraft::from_config(&config));
    drop(app.update(Message::Scene(crate::SceneMessage::BeginAdd)));
    drop(
        app.update(Message::Scene(crate::SceneMessage::EditorChanged {
            field: crate::SceneEditorField::Label,
            value: "Unsaved meeting scene".to_owned(),
        })),
    );
    let editor_before = format!("{:?}", app.scene_editor);

    drop(app.begin_script_install(LiveScriptKind::AsrProvider, "fixture".to_owned()));

    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("open Scene form")
    ));
    assert!(matches!(app.script_install, ScriptInstallState::Idle));
    assert_eq!(format!("{:?}", app.scene_editor), editor_before);
    assert!(editor_before.contains("Unsaved meeting scene"));
}

#[test]
fn open_llm_provider_editor_blocks_adapter_changes_without_losing_input() {
    let mut app = crate::test_support::GuiHarness::new();
    let config = vinpst_config::VinpstConfig::bundled_default().expect("bundled config");
    app.config = Ok(ConfigDocument {
        path: "/tmp/vinpst-gui-provider-adapter-draft.json".into(),
        from_disk: false,
        config: config.clone(),
    });
    app.draft = Some(crate::ConfigDraft::from_config(&config));
    drop(app.update(Message::LlmProvider(crate::LlmProviderMessage::BeginAdd)));
    drop(app.update(Message::LlmProvider(
        crate::LlmProviderMessage::EditorChanged {
            field: crate::LlmProviderEditorField::Id,
            value: SecretInput::new("unsaved-provider".to_owned()),
        },
    )));
    let editor_before = format!("{:?}", app.llm_provider_editor);

    drop(app.begin_script_install(LiveScriptKind::LlmAdapter, "fixture".to_owned()));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("open LLM provider form")
    ));
    assert!(matches!(app.script_install, ScriptInstallState::Idle));
    assert_eq!(format!("{:?}", app.llm_provider_editor), editor_before);

    app.operation = OperationState::Idle;
    drop(app.begin_script_remove(
        LiveScriptKind::LlmAdapter,
        "adapter.fixture.command".to_owned(),
    ));
    assert!(matches!(
        app.operation,
        OperationState::Failed(ref error) if error.contains("open LLM provider form")
    ));
    assert_eq!(format!("{:?}", app.llm_provider_editor), editor_before);
    assert!(editor_before.contains("unsaved-provider"));
}
