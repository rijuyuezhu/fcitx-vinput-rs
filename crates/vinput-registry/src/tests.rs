use super::{
    ChecksumPolicy, InstallPlan, RegistryCacheError, RegistryCachedFetchError, RegistryError,
    RegistryFetchError, RegistryIndex, RegistrySha256Error, RegistryTextCache, RegistryTextSource,
    ReqwestRegistryTextSource, fetch_registry_index_from_mirrors, fetch_registry_index_with_cache,
    sha256_hex, verify_sha256_bytes, verify_sha256_file, verify_sha256_reader,
};
use vinput_config::RegistryConfig;

#[derive(Debug, Default)]
struct StaticRegistryTextSource {
    responses: std::collections::HashMap<String, Result<String, String>>,
    attempts: std::sync::Mutex<Vec<String>>,
}

impl StaticRegistryTextSource {
    fn with_response(mut self, url: &str, response: Result<&str, &str>) -> Self {
        self.responses.insert(
            url.to_owned(),
            response.map(str::to_owned).map_err(str::to_owned),
        );
        self
    }

    fn attempts(&self) -> Vec<String> {
        self.attempts.lock().unwrap().clone()
    }
}

impl RegistryTextSource for StaticRegistryTextSource {
    fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
        self.attempts.lock().unwrap().push(url.to_owned());
        self.responses
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err("not configured".to_owned()))
    }
}

const SAMPLE: &str = r#"
    {
      "version": 1,
      "models": [
        {
          "id": "sherpa-zh-small",
          "label": "Sherpa zh small",
          "provider": "sherpa-onnx",
          "language": "zh",
          "assets": [
            {
              "path": "models/sherpa-zh-small.tar.zst",
              "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "size_bytes": 42
            }
          ]
        }
      ],
      "adapters": [
        {
          "id": "mock-adapter",
          "label": "Mock adapter",
          "kind": "command",
          "assets": [
            {
              "path": "adapters/mock-adapter.tar.zst",
              "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
              "size_bytes": 7
            }
          ]
        }
      ]
    }
    "#;

#[test]
fn rejects_missing_registry_version() {
    let error = RegistryIndex::from_json_str(r#"{"models":[]}"#).unwrap_err();

    assert!(matches!(error, RegistryError::Json(_)));
}

#[test]
fn rejects_zero_registry_version() {
    assert_eq!(
        RegistryIndex::from_json_str(r#"{"version":0}"#).unwrap_err(),
        RegistryError::InvalidVersion
    );
}

#[test]
fn parses_and_finds_registry_entries() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    assert_eq!(index.version, 1);
    assert_eq!(
        index.model("sherpa-zh-small").unwrap().provider,
        "sherpa-onnx"
    );
    assert_eq!(index.adapter("mock-adapter").unwrap().kind, "command");
    assert!(index.model("missing").is_none());
}

#[test]
fn summarizes_planned_asset_sizes() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let plan = index.planned_assets(&RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    });
    let summary = super::AssetPlanSummary::from_assets(&plan);
    assert_eq!(summary.asset_count, 2);
    assert_eq!(summary.known_size_bytes, 49);
    assert_eq!(summary.unknown_size_count, 0);
}

#[test]
fn summary_counts_registry_entries_and_assets() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let summary = index.summary();
    assert_eq!(summary.version, 1);
    assert_eq!(summary.model_count, 1);
    assert_eq!(summary.adapter_count, 1);
    assert_eq!(summary.asset_count, 2);
}

#[test]
fn planned_assets_preserve_manifest_entry_order() {
    let index = RegistryIndex::from_json_str(
        r#"{
              "version": 1,
              "models": [
                {"id":"m1","label":"M1","provider":"p","assets":[{"path":"models/m1.tar"}]},
                {"id":"m2","label":"M2","provider":"p","assets":[{"path":"models/m2.tar"}]}
              ],
              "adapters": [
                {"id":"a1","label":"A1","kind":"command","assets":[{"path":"adapters/a1.tar"}]},
                {"id":"a2","label":"A2","kind":"command","assets":[{"path":"adapters/a2.tar"}]}
              ]
            }"#,
    )
    .unwrap();

    let plan = index.planned_assets(&RegistryConfig {
        base_urls: vec!["mirror".to_owned()],
    });

    assert_eq!(
        plan.iter()
            .map(|asset| asset.entry_id.as_str())
            .collect::<Vec<_>>(),
        ["m1", "m2", "a1", "a2"]
    );
    assert_eq!(
        plan.iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>(),
        [
            "models/m1.tar",
            "models/m2.tar",
            "adapters/a1.tar",
            "adapters/a2.tar"
        ]
    );
}

#[test]
fn plans_assets_with_entry_metadata_and_urls() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let plan = index.planned_assets(&RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    });
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].entry_kind, super::RegistryEntryKind::Model);
    assert_eq!(plan[0].entry_id, "sherpa-zh-small");
    assert_eq!(plan[0].path, "models/sherpa-zh-small.tar.zst");
    assert_eq!(
        plan[0].urls,
        vec!["https://registry.invalid/root/models/sherpa-zh-small.tar.zst".to_owned()]
    );
    assert_eq!(plan[1].entry_kind, super::RegistryEntryKind::Adapter);
    assert_eq!(plan[1].entry_id, "mock-adapter");
    assert_eq!(
        plan[1].urls,
        vec!["https://registry.invalid/root/adapters/mock-adapter.tar.zst".to_owned()]
    );
}
#[test]
fn install_plan_adds_targets_and_checksum_policy() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let config = RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    };
    let plan = index.install_plan(&config, "/var/lib/vinput/assets/");

    assert_eq!(plan.target_root, "/var/lib/vinput/assets");
    assert_eq!(plan.summary.asset_count, 2);
    assert_eq!(plan.summary.known_size_bytes, 49);
    assert_eq!(plan.summary.missing_checksum_count, 0);
    assert_eq!(
        plan.assets[0].target_path,
        "/var/lib/vinput/assets/models/sherpa-zh-small.tar.zst"
    );
    assert_eq!(plan.assets[0].checksum_policy, ChecksumPolicy::Sha256);
}

#[test]
fn selected_install_plans_filter_entries() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let config = RegistryConfig {
        base_urls: vec!["mirror".to_owned()],
    };

    let model_plan = index
        .install_model_plan("sherpa-zh-small", &config, "cache")
        .unwrap();
    assert_eq!(model_plan.summary.asset_count, 1);
    assert_eq!(model_plan.assets[0].entry_id, "sherpa-zh-small");
    assert_eq!(
        model_plan.assets[0].target_path,
        "cache/models/sherpa-zh-small.tar.zst"
    );

    let adapter_plan = index
        .install_adapter_plan("mock-adapter", &config, "cache")
        .unwrap();
    assert_eq!(adapter_plan.summary.asset_count, 1);
    assert_eq!(adapter_plan.assets[0].entry_id, "mock-adapter");
    assert_eq!(
        adapter_plan.assets[0].target_path,
        "cache/adapters/mock-adapter.tar.zst"
    );
}

#[test]
fn install_plan_preserves_multiple_mirror_urls() {
    let index = RegistryIndex::from_json_str(
            r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"a.bin"}]}]}"#,
        )
        .unwrap();
    let plan = index.install_plan(
        &RegistryConfig {
            base_urls: vec!["m1".to_owned(), "m2".to_owned()],
        },
        "cache",
    );

    assert_eq!(plan.assets[0].urls, ["m1/a.bin", "m2/a.bin"]);
}

#[test]
fn install_plan_keeps_assets_without_urls() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let plan = index.install_plan(
        &RegistryConfig {
            base_urls: Vec::new(),
        },
        "cache",
    );

    assert_eq!(plan.summary.asset_count, 2);
    assert!(plan.assets.iter().all(|asset| asset.urls.is_empty()));
}

#[test]
fn install_plan_summarizes_empty_indexes() {
    let index = RegistryIndex::from_json_str(r#"{"version":1}"#).unwrap();
    let plan = index.install_plan(
        &RegistryConfig {
            base_urls: vec!["m".to_owned()],
        },
        "cache",
    );

    assert_eq!(plan.summary.asset_count, 0);
    assert_eq!(plan.summary.known_size_bytes, 0);
    assert_eq!(plan.summary.missing_checksum_count, 0);
    assert!(plan.assets.is_empty());
}

#[test]
fn install_plan_tracks_missing_checksums() {
    let index = RegistryIndex::from_json_str(
            r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}]}"#,
        )
        .unwrap();
    let assets = index.planned_assets(&RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    });
    let plan = InstallPlan::from_assets(&assets, "cache");

    assert_eq!(plan.summary.missing_checksum_count, 1);
    assert_eq!(plan.assets[0].target_path, "cache/models/m.tar");
    assert_eq!(plan.assets[0].checksum_policy, ChecksumPolicy::Missing);
}

#[test]
fn install_plan_uses_relative_targets_for_empty_root() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let config = RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    };
    let plan = index.install_plan(&config, "");

    assert_eq!(plan.target_root, "");
    assert_eq!(plan.assets[0].target_path, "models/sherpa-zh-small.tar.zst");
    assert_eq!(plan.assets[1].target_path, "adapters/mock-adapter.tar.zst");
}

#[test]
fn resolves_no_urls_when_registry_has_no_base_urls() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let asset = &index.model("sherpa-zh-small").unwrap().assets[0];

    assert!(
        asset
            .resolved_urls(&RegistryConfig { base_urls: vec![] })
            .is_empty()
    );
}

#[test]
fn resolves_asset_against_all_base_urls() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let asset = &index.model("sherpa-zh-small").unwrap().assets[0];
    let urls = asset.resolved_urls(&RegistryConfig {
        base_urls: vec![
            "https://example.invalid/root/".to_owned(),
            "https://mirror.invalid/root".to_owned(),
        ],
    });
    assert_eq!(
        urls,
        vec![
            "https://example.invalid/root/models/sherpa-zh-small.tar.zst".to_owned(),
            "https://mirror.invalid/root/models/sherpa-zh-small.tar.zst".to_owned(),
        ]
    );
}

#[test]
fn plans_assets_for_selected_entries() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let config = RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    };
    let model_plan = index
        .planned_model_assets("sherpa-zh-small", &config)
        .unwrap();
    assert_eq!(model_plan.len(), 1);
    assert_eq!(model_plan[0].entry_kind, super::RegistryEntryKind::Model);
    assert_eq!(model_plan[0].entry_id, "sherpa-zh-small");
    let adapter_plan = index
        .planned_adapter_assets("mock-adapter", &config)
        .unwrap();
    assert_eq!(adapter_plan.len(), 1);
    assert_eq!(
        adapter_plan[0].entry_kind,
        super::RegistryEntryKind::Adapter
    );
    assert_eq!(adapter_plan[0].entry_id, "mock-adapter");
}

#[test]
fn selected_asset_plans_reject_unknown_entries() {
    let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
    let config = RegistryConfig {
        base_urls: vec!["https://registry.invalid/root".to_owned()],
    };
    assert_eq!(
        index
            .planned_model_assets("missing-model", &config)
            .unwrap_err(),
        RegistryError::UnknownModelId("missing-model".to_owned())
    );
    assert_eq!(
        index
            .planned_adapter_assets("missing-adapter", &config)
            .unwrap_err(),
        RegistryError::UnknownAdapterId("missing-adapter".to_owned())
    );
}

#[test]
fn rejects_empty_model_ids() {
    let json = r#"{"version":1,"models":[{"id":" ","label":"M","provider":"p","assets":[]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::EmptyId
    );
}

#[test]
fn rejects_empty_model_providers() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":" ","assets":[]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::EmptyProvider("m".to_owned())
    );
}

#[test]
fn rejects_empty_adapter_kinds() {
    let json = r#"{"version":1,"adapters":[{"id":"a","label":"A","kind":" ","assets":[]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::EmptyAdapterKind("a".to_owned())
    );
}

#[test]
fn rejects_duplicate_model_ids() {
    let json = r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[]},
            {"id":"m","label":"M again","provider":"p","assets":[]}
          ]
        }
        "#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::DuplicateModelId("m".to_owned())
    );
}

#[test]
fn rejects_duplicate_adapter_ids() {
    let json = r#"
        {
          "version": 1,
          "adapters": [
            {"id":"a","label":"A","kind":"command","assets":[]},
            {"id":"a","label":"A again","kind":"command","assets":[]}
          ]
        }
        "#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::DuplicateAdapterId("a".to_owned())
    );
}

#[test]
fn rejects_empty_asset_paths() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"   "}]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::EmptyAssetPath
    );
}

#[test]
fn rejects_url_like_asset_paths() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"https://example.invalid/model.tar"}]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::UnsafeAssetPath("https://example.invalid/model.tar".to_owned())
    );
}

#[test]
fn rejects_backslash_asset_paths() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"models\\m.tar"}]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::UnsafeAssetPath("models\\m.tar".to_owned())
    );
}

#[test]
fn rejects_absolute_asset_paths() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"/absolute/model.tar"}]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::UnsafeAssetPath("/absolute/model.tar".to_owned())
    );
}

#[test]
fn rejects_unsafe_asset_paths() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"../secret"}]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::UnsafeAssetPath("../secret".to_owned())
    );
}

#[test]
fn rejects_duplicate_asset_paths_within_entry() {
    let json = r#"
        {
          "version": 1,
          "models": [
            {
              "id":"m",
              "label":"M",
              "provider":"p",
              "assets":[{"path":"m.tar"},{"path":"m.tar"}]
            }
          ]
        }
        "#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::DuplicateAssetPath("m.tar".to_owned())
    );
}

#[test]
fn rejects_invalid_sha256() {
    let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"m.tar","sha256":"ABC"}]}]}"#;
    assert_eq!(
        RegistryIndex::from_json_str(json).unwrap_err(),
        RegistryError::InvalidSha256("ABC".to_owned())
    );
}

#[test]
fn fetch_registry_index_uses_first_successful_mirror() {
    let source = StaticRegistryTextSource::default()
        .with_response("https://first.invalid/index.json", Err("offline"))
        .with_response("https://second.invalid/index.json", Ok(SAMPLE));
    let mirrors = vec![
        "https://first.invalid/index.json".to_owned(),
        "https://second.invalid/index.json".to_owned(),
        "https://third.invalid/index.json".to_owned(),
    ];

    let index = fetch_registry_index_from_mirrors(&source, &mirrors).unwrap();

    assert_eq!(index.summary().model_count, 1);
    assert_eq!(
        source.attempts(),
        [
            "https://first.invalid/index.json".to_owned(),
            "https://second.invalid/index.json".to_owned(),
        ]
    );
}

#[test]
fn fetch_registry_index_reports_all_mirror_failures() {
    let source = StaticRegistryTextSource::default()
        .with_response("https://first.invalid/index.json", Err("offline"))
        .with_response("https://second.invalid/index.json", Err("timeout"));
    let mirrors = vec![
        "https://first.invalid/index.json".to_owned(),
        "https://second.invalid/index.json".to_owned(),
    ];

    let error = fetch_registry_index_from_mirrors(&source, &mirrors).unwrap_err();

    let RegistryFetchError::AllMirrorsFailed(failures) = error else {
        panic!("expected all mirrors failed error");
    };
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0].url, "https://first.invalid/index.json");
    assert_eq!(failures[0].message, "offline");
    assert_eq!(failures[1].url, "https://second.invalid/index.json");
    assert_eq!(failures[1].message, "timeout");
}

#[test]
fn fetch_registry_index_rejects_empty_mirror_list() {
    let source = StaticRegistryTextSource::default();

    assert_eq!(
        fetch_registry_index_from_mirrors(&source, &[]),
        Err(RegistryFetchError::NoMirrors)
    );
}

#[test]
fn fetch_registry_index_stops_on_invalid_successful_mirror() {
    let source = StaticRegistryTextSource::default()
        .with_response("https://first.invalid/index.json", Ok(r#"{"version":0}"#))
        .with_response("https://second.invalid/index.json", Ok(SAMPLE));
    let mirrors = vec![
        "https://first.invalid/index.json".to_owned(),
        "https://second.invalid/index.json".to_owned(),
    ];

    let error = fetch_registry_index_from_mirrors(&source, &mirrors).unwrap_err();

    assert!(matches!(
        error,
        RegistryFetchError::InvalidIndex { url, error: RegistryError::InvalidVersion }
            if url == "https://first.invalid/index.json"
    ));
    assert_eq!(
        source.attempts(),
        ["https://first.invalid/index.json".to_owned()]
    );
}

#[test]
fn registry_text_cache_writes_fresh_fetch_atomically() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = RegistryTextCache::new(temp_dir.path().join("nested/index.json"));
    let source = StaticRegistryTextSource::default()
        .with_response("https://mirror.invalid/index.json", Ok(SAMPLE));

    let index = fetch_registry_index_with_cache(
        &source,
        &["https://mirror.invalid/index.json".to_owned()],
        &cache,
    )
    .unwrap();

    assert_eq!(index.summary().model_count, 1);
    assert_eq!(std::fs::read_to_string(cache.path()).unwrap(), SAMPLE);
    assert_eq!(cache.read_index().unwrap().summary().asset_count, 2);
    let cache_dir = cache.path().parent().unwrap();
    let temp_entries = std::fs::read_dir(cache_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp."))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "temp cache files left behind: {temp_entries:?}"
    );
}

#[test]
fn registry_text_cache_uses_stale_cache_after_fetch_failure() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = RegistryTextCache::new(temp_dir.path().join("index.json"));
    std::fs::write(cache.path(), SAMPLE).unwrap();
    let source = StaticRegistryTextSource::default()
        .with_response("https://mirror.invalid/index.json", Err("offline"));

    let index = fetch_registry_index_with_cache(
        &source,
        &["https://mirror.invalid/index.json".to_owned()],
        &cache,
    )
    .unwrap();

    assert_eq!(index.summary().model_count, 1);
    assert_eq!(
        source.attempts(),
        ["https://mirror.invalid/index.json".to_owned()]
    );
}

#[test]
fn registry_text_cache_reports_invalid_stale_cache_after_fetch_failure() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = RegistryTextCache::new(temp_dir.path().join("index.json"));
    std::fs::write(cache.path(), r#"{"version":0}"#).unwrap();
    let source = StaticRegistryTextSource::default()
        .with_response("https://mirror.invalid/index.json", Err("offline"));

    let error = fetch_registry_index_with_cache(
        &source,
        &["https://mirror.invalid/index.json".to_owned()],
        &cache,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        RegistryCachedFetchError::StaleCacheUnavailable {
            fetch: RegistryFetchError::AllMirrorsFailed(_),
            cache: RegistryCacheError::InvalidIndex {
                error: RegistryError::InvalidVersion,
                ..
            },
        }
    ));
}

#[test]
fn registry_text_cache_does_not_treat_partial_temp_file_as_success() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache = RegistryTextCache::new(temp_dir.path().join("index.json"));
    std::fs::write(temp_dir.path().join(".index.json.tmp.manual"), SAMPLE).unwrap();
    let source = StaticRegistryTextSource::default()
        .with_response("https://mirror.invalid/index.json", Err("offline"));

    let error = fetch_registry_index_with_cache(
        &source,
        &["https://mirror.invalid/index.json".to_owned()],
        &cache,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        RegistryCachedFetchError::StaleCacheUnavailable {
            fetch: RegistryFetchError::AllMirrorsFailed(_),
            cache: RegistryCacheError::Read { .. },
        }
    ));
}

const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[test]
fn sha256_helper_verifies_bytes() {
    assert_eq!(sha256_hex(b"hello"), HELLO_SHA256);
    verify_sha256_bytes(b"hello", HELLO_SHA256).unwrap();
}

#[test]
fn sha256_helper_reports_mismatch() {
    let error = verify_sha256_bytes(
        b"hello",
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .unwrap_err();

    assert_eq!(
        error,
        RegistrySha256Error::Mismatch {
            expected: "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
            actual: HELLO_SHA256.to_owned(),
        }
    );
}

#[test]
fn sha256_helper_rejects_invalid_expected_checksum() {
    assert_eq!(
        verify_sha256_bytes(b"hello", "ABC").unwrap_err(),
        RegistrySha256Error::InvalidExpected("ABC".to_owned())
    );
    assert_eq!(
        verify_sha256_bytes(
            b"hello",
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824",
        )
        .unwrap_err(),
        RegistrySha256Error::InvalidExpected(
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824".to_owned()
        )
    );
}

#[test]
fn sha256_helper_verifies_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("asset.bin");
    std::fs::write(&path, b"hello").unwrap();

    verify_sha256_file(&path, HELLO_SHA256).unwrap();
}

#[test]
fn sha256_helper_reports_file_open_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("missing.bin");

    let error = verify_sha256_file(&path, HELLO_SHA256).unwrap_err();

    assert!(matches!(error, RegistrySha256Error::OpenFile { .. }));
}

#[derive(Debug)]
struct FailingReader;

impl std::io::Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("secret-reader-detail"))
    }
}

#[test]
fn sha256_helper_reports_reader_error_without_details() {
    let error = verify_sha256_reader(FailingReader, HELLO_SHA256).unwrap_err();

    assert_eq!(
        error,
        RegistrySha256Error::Read {
            message: "other error".to_owned(),
        }
    );
}

#[derive(Debug)]
struct CapturedRegistryHttpRequest {
    head: String,
}

fn serve_registry_http_response(
    status: &str,
    response_body: &str,
) -> (String, std::thread::JoinHandle<CapturedRegistryHttpRequest>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let status = status.to_owned();
    let response_body = response_body.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let head = read_registry_http_request_head(&mut stream);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        CapturedRegistryHttpRequest { head }
    });
    (url, handle)
}

fn read_registry_http_request_head(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = std::io::Read::read(stream, &mut chunk).unwrap();
        assert_ne!(read, 0, "HTTP client closed before headers were complete");
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            return String::from_utf8_lossy(&buffer[..position + 4]).into_owned();
        }
    }
}

fn closed_local_http_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

#[test]
fn reqwest_registry_text_source_fetches_and_parses_http_200() {
    let (url, handle) = serve_registry_http_response("200 OK", SAMPLE);
    let source = ReqwestRegistryTextSource::new();

    let index = fetch_registry_index_from_mirrors(&source, &[url]).unwrap();

    assert_eq!(index.summary().model_count, 1);
    let request = handle.join().unwrap();
    assert!(request.head.starts_with("GET / HTTP/1.1"));
    assert!(!request.head.to_ascii_lowercase().contains("authorization"));
}

#[test]
fn reqwest_registry_text_source_sanitizes_http_error_body() {
    let (url, handle) = serve_registry_http_response("500 Internal Server Error", "secret-token");
    let source = ReqwestRegistryTextSource::new();

    let message = source.fetch_registry_text(&url).unwrap_err();

    assert!(message.contains("HTTP 500 Internal Server Error"));
    assert!(!message.contains("secret-token"));
    assert!(!message.contains(&url));
    handle.join().unwrap();
}

#[test]
fn reqwest_registry_text_source_sanitizes_timeout_or_connection_failure() {
    let url = closed_local_http_url();
    let source = ReqwestRegistryTextSource::with_timeout(std::time::Duration::from_millis(250));

    let message = source.fetch_registry_text(&url).unwrap_err();

    assert!(
        [
            "registry HTTP connection failed",
            "registry HTTP request timed out",
        ]
        .contains(&message.as_str())
    );
    assert!(!message.contains(&url));
}

#[test]
fn reqwest_registry_text_source_keeps_mirror_fallback_in_fetch_boundary() {
    let (first_url, first_handle) =
        serve_registry_http_response("503 Service Unavailable", "try later");
    let (second_url, second_handle) = serve_registry_http_response("200 OK", SAMPLE);
    let source = ReqwestRegistryTextSource::new();

    let index = fetch_registry_index_from_mirrors(&source, &[first_url, second_url]).unwrap();

    assert_eq!(index.summary().model_count, 1);
    first_handle.join().unwrap();
    second_handle.join().unwrap();
}
