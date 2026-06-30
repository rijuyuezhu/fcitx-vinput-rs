//! Integration coverage for registry staging path planning.

use vinput_registry::{
    InstallPlan, PlannedAsset, RegistryEntryKind, plan_archive_staging_paths_for_plan,
};

#[test]
fn batch_rejects_same_extract_tree() {
    let assets = vec![
        PlannedAsset {
            entry_kind: RegistryEntryKind::Model,
            entry_id: "plain".to_owned(),
            path: "models/shared.tar".to_owned(),
            urls: Vec::new(),
            sha256: None,
            size_bytes: None,
        },
        PlannedAsset {
            entry_kind: RegistryEntryKind::Model,
            entry_id: "compressed".to_owned(),
            path: "models/shared.tar.zst".to_owned(),
            urls: Vec::new(),
            sha256: None,
            size_bytes: None,
        },
    ];
    let plan = InstallPlan::from_assets(&assets, "");

    assert!(plan_archive_staging_paths_for_plan(&plan, "stage-root").is_err());
}
