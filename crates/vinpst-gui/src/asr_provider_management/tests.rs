use std::collections::HashMap;

use super::*;

fn command_provider() -> AsrProviderConfig {
    AsrProviderConfig {
        id: "command-provider".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(4_000),
        model: Some("model-a".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some("/usr/bin/provider".to_owned()),
        args: vec!["--json".to_owned()],
        env: HashMap::from([
            ("OTHER".to_owned(), "keep".to_owned()),
            ("TOKEN".to_owned(), "secret".to_owned()),
        ]),
        endpoint: None,
    }
}

#[test]
fn command_editor_preserves_identity_and_hotword_while_updating_typed_fields() {
    let original = command_provider();
    let mut editor = AsrProviderEditorState::edit(&original);
    editor.update(
        AsrProviderEditorField::TimeoutMs,
        SecretInput::new("9000".to_owned()),
    );
    editor.update(
        AsrProviderEditorField::Command,
        SecretInput::new(" /opt/provider ".to_owned()),
    );
    editor.update(
        AsrProviderEditorField::Args,
        SecretInput::new("[\"--stream\", \"--lang=en\"]".to_owned()),
    );
    let token_index = editor
        .fields
        .environment
        .iter()
        .position(|entry| entry.key == "TOKEN")
        .expect("TOKEN row");
    editor.update_environment_key(token_index, "API_KEY".to_owned());
    editor.update_environment_value(token_index, SecretInput::new("value".to_owned()));

    let provider = editor.provider().expect("provider should validate");
    assert_eq!(provider.id, original.id);
    assert_eq!(provider.kind, original.kind);
    assert_eq!(provider.hotwords_file, original.hotwords_file);
    assert_eq!(provider.timeout_ms, Some(9_000));
    assert_eq!(provider.command.as_deref(), Some("/opt/provider"));
    assert_eq!(provider.args, ["--stream", "--lang=en"]);
    assert_eq!(
        provider.env.get("API_KEY").map(String::as_str),
        Some("value")
    );
    assert_eq!(provider.env.get("OTHER").map(String::as_str), Some("keep"));
}

#[test]
fn add_builds_kind_specific_providers_and_rejects_duplicates() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config should validate");

    let mut command = AsrProviderEditorState::add();
    assert!(!command.is_dirty());
    command.update(
        AsrProviderEditorField::Id,
        SecretInput::new(" custom-command ".to_owned()),
    );
    command.update(
        AsrProviderEditorField::Command,
        SecretInput::new(" /opt/custom-provider ".to_owned()),
    );
    command.update(
        AsrProviderEditorField::Args,
        SecretInput::new(r#"["--json"]"#.to_owned()),
    );
    command.add_environment();
    command.update_environment_key(0, "TOKEN".to_owned());
    command.update_environment_value(0, SecretInput::new("secret".to_owned()));
    let command_provider = command
        .provider()
        .expect("command provider should validate");
    assert_eq!(command_provider.id, "custom-command");
    assert_eq!(command_provider.kind, AsrProviderKind::Command);
    assert_eq!(
        command_provider.command.as_deref(),
        Some("/opt/custom-provider")
    );
    assert_eq!(command_provider.args, ["--json"]);
    assert_eq!(
        command_provider.env.get("TOKEN").map(String::as_str),
        Some("secret")
    );
    assert!(command_provider.endpoint.is_none());

    config = upsert_asr_provider(&config, &command).expect("new command provider should persist");
    assert!(upsert_asr_provider(&config, &command).is_err());

    let mut local = AsrProviderEditorState::add();
    local.set_kind(AsrProviderKind::Local);
    local.update(
        AsrProviderEditorField::Id,
        SecretInput::new("local-provider".to_owned()),
    );
    local.update(
        AsrProviderEditorField::Model,
        SecretInput::new(" /models/asr ".to_owned()),
    );
    local.update(
        AsrProviderEditorField::Command,
        SecretInput::new("ignored-command".to_owned()),
    );
    local.update(
        AsrProviderEditorField::Endpoint,
        SecretInput::new("https://ignored.invalid".to_owned()),
    );
    let local_provider = local.provider().expect("local provider should validate");
    assert_eq!(local_provider.kind, AsrProviderKind::Local);
    assert_eq!(local_provider.model.as_deref(), Some("/models/asr"));
    assert!(local_provider.command.is_none());
    assert!(local_provider.args.is_empty());
    assert!(local_provider.env.is_empty());
    assert!(local_provider.endpoint.is_none());

    let mut remote = AsrProviderEditorState::add();
    remote.set_kind(AsrProviderKind::Remote);
    remote.update(
        AsrProviderEditorField::Id,
        SecretInput::new("remote-provider".to_owned()),
    );
    remote.update(
        AsrProviderEditorField::Endpoint,
        SecretInput::new(" https://example.invalid/asr ".to_owned()),
    );
    remote.update(
        AsrProviderEditorField::Command,
        SecretInput::new("ignored-command".to_owned()),
    );
    remote.update(
        AsrProviderEditorField::Args,
        SecretInput::new(r#"["ignored"]"#.to_owned()),
    );
    let remote_provider = remote.provider().expect("remote provider should validate");
    assert_eq!(remote_provider.kind, AsrProviderKind::Remote);
    assert_eq!(
        remote_provider.endpoint.as_deref(),
        Some("https://example.invalid/asr")
    );
    assert!(remote_provider.command.is_none());
    assert!(remote_provider.args.is_empty());
    assert!(remote_provider.env.is_empty());
}

#[test]
fn custom_provider_removal_is_config_only_and_rejects_active_or_managed_entries() {
    let provider = command_provider();
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.asr.providers.push(provider.clone());

    let updated = remove_custom_asr_provider_config_with(&config, &provider.id, |_| false)
        .expect("inactive custom provider should be removed");
    assert!(
        updated
            .asr
            .providers
            .iter()
            .all(|candidate| candidate.id != provider.id)
    );

    assert!(remove_custom_asr_provider_config_with(&config, &provider.id, |_| true).is_err());
    config.asr.active_provider = provider.id.clone();
    assert!(remove_custom_asr_provider_config_with(&config, &provider.id, |_| false).is_err());
    assert!(remove_custom_asr_provider_config_with(&config, "missing", |_| false).is_err());
}

#[test]
fn edit_mode_ignores_identity_and_kind_messages() {
    let original = command_provider();
    let mut editor = AsrProviderEditorState::edit(&original);
    editor.update(
        AsrProviderEditorField::Id,
        SecretInput::new("forged-id".to_owned()),
    );
    editor.set_kind(AsrProviderKind::Remote);

    let provider = editor
        .provider()
        .expect("edited provider should remain valid");
    assert_eq!(provider.id, original.id);
    assert_eq!(provider.kind, original.kind);
}

#[test]
fn provider_editor_rejects_invalid_timeout_args_environment_and_required_targets() {
    assert!(parse_optional_timeout("0").is_err());
    assert!(parse_optional_timeout("1.5").is_err());
    assert!(parse_string_array("{\"not\":\"array\"}", "arguments").is_err());
    assert!(
        environment_map(&[AsrProviderEnvironmentEntry {
            key: "   ".to_owned(),
            value: SecretInput::new("value".to_owned()),
        }])
        .is_err()
    );
    assert!(
        environment_map(&[
            AsrProviderEnvironmentEntry {
                key: "DUPLICATE".to_owned(),
                value: SecretInput::new("one".to_owned()),
            },
            AsrProviderEnvironmentEntry {
                key: "DUPLICATE".to_owned(),
                value: SecretInput::new("two".to_owned()),
            },
        ])
        .is_err()
    );

    let mut command = AsrProviderEditorState::edit(&command_provider());
    command.update(
        AsrProviderEditorField::Command,
        SecretInput::new("   ".to_owned()),
    );
    assert!(command.provider().is_err());

    let mut remote = command_provider();
    remote.kind = AsrProviderKind::Remote;
    remote.command = None;
    remote.args.clear();
    remote.env.clear();
    remote.endpoint = Some("https://example.invalid/asr".to_owned());
    let mut remote = AsrProviderEditorState::edit(&remote);
    remote.update(
        AsrProviderEditorField::Endpoint,
        SecretInput::new(String::new()),
    );
    assert!(remote.provider().is_err());
}

#[test]
fn provider_editor_debug_redacts_command_arguments_environment_and_endpoint_secrets() {
    let mut provider = command_provider();
    provider.command = Some("/secret/path/provider".to_owned());
    provider.args = vec!["--token=argument-secret".to_owned()];
    provider
        .env
        .insert("KEY".to_owned(), "environment-secret".to_owned());
    provider.endpoint = Some("https://user:pass@example.invalid/asr?token=query-secret".to_owned());
    let debug = format!("{:?}", AsrProviderEditorState::edit(&provider));
    assert!(!debug.contains("/secret/path/provider"));
    assert!(!debug.contains("argument-secret"));
    assert!(!debug.contains("environment-secret"));
    assert!(!debug.contains("query-secret"));
    assert!(!debug.contains("pass"));

    let message = AsrProviderMessage::EnvironmentValueChanged {
        index: 0,
        value: SecretInput::new("message-secret".to_owned()),
    };
    assert!(!format!("{message:?}").contains("message-secret"));
}

#[test]
fn edit_rejects_stale_provider_and_validates_complete_config() {
    let provider = command_provider();
    let editor = AsrProviderEditorState::edit(&provider);
    let mut config = VinpstConfig::bundled_default().expect("bundled config should validate");
    config.asr.providers = vec![provider.clone()];
    config.asr.active_provider = provider.id.clone();

    let updated = upsert_asr_provider(&config, &editor).expect("unchanged provider is valid");
    assert_eq!(
        updated.asr.providers.as_slice(),
        std::slice::from_ref(&provider)
    );

    let mut stale = config;
    stale.asr.providers[0].timeout_ms = Some(8_000);
    assert!(upsert_asr_provider(&stale, &editor).is_err());
}

#[test]
fn dirty_provider_form_blocks_navigation_and_resource_mutations() {
    let mut app = crate::test_support::GuiHarness::new();
    let provider = app
        .config
        .as_ref()
        .expect("bundled config should load")
        .config
        .asr
        .providers
        .first()
        .expect("bundled config should include a provider")
        .clone();
    app.page = crate::Page::Resources;
    app.asr_provider_editor = Some(AsrProviderEditorState::edit(&provider));
    app.asr_provider_editor
        .as_mut()
        .expect("editor should remain open")
        .update(
            AsrProviderEditorField::TimeoutMs,
            SecretInput::new("12345".to_owned()),
        );

    let _ = app.update(Message::SelectPage(crate::Page::Llm));
    assert_eq!(app.page, crate::Page::Resources);
    assert!(app.asr_provider_editor.is_some());

    let _ = app.update(Message::InstallRegistryModel("fixture-model".to_owned()));
    assert!(app.asr_provider_editor.is_some());
    assert!(matches!(app.operation, OperationState::Failed(_)));
}
