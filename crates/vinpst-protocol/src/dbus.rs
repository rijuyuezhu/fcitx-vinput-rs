//! D-Bus names that must remain compatible with the C++ fcitx5-vinpst addon.

/// Well-known bus name owned by Fcitx5.
pub const FCITX_BUS_NAME: &str = "org.fcitx.Fcitx5";

/// Well-known bus name owned by the Rust daemon.
pub const SERVICE_BUS_NAME: &str = "org.fcitx.Vinpst";

/// Object path exported by the Rust daemon.
pub const SERVICE_OBJECT_PATH: &str = "/org/fcitx/Vinpst";

/// Main service interface exported by the Rust daemon.
pub const SERVICE_INTERFACE: &str = "org.fcitx.Vinpst.Service";

/// Object path used by the Fcitx5 addon-side notifier.
pub const FRONTEND_NOTIFIER_OBJECT_PATH: &str = "/org/fcitx/Fcitx5/Vinpst";

/// Interface used by the Fcitx5 addon-side notifier.
pub const FRONTEND_NOTIFIER_INTERFACE: &str = "org.fcitx.Fcitx5.Vinpst1";

/// Method names on [`SERVICE_INTERFACE`].
pub mod method {
    /// Start normal speech recognition.
    pub const START_RECORDING: &str = "StartRecording";
    /// Start command-mode speech recognition with selected text context.
    pub const START_COMMAND_RECORDING: &str = "StartCommandRecording";
    /// Stop the current recording and produce a recognition result.
    pub const STOP_RECORDING: &str = "StopRecording";
    /// Return the current daemon status string.
    pub const GET_STATUS: &str = "GetStatus";
    /// Return the legacy selected/effective ASR backend tuple.
    pub const GET_ASR_BACKEND_STATE: &str = "GetAsrBackendState";
    /// Return a JSON snapshot of configured text adapters.
    pub const GET_TEXT_ADAPTER_STATE: &str = "GetTextAdapterState";
    /// Return a JSON snapshot of current runtime status diagnostics.
    pub const GET_RUNTIME_STATUS: &str = "GetRuntimeStatus";
    /// Return the active scene and configured scene id/label pairs.
    pub const GET_SCENE_STATE: &str = "GetSceneState";
    /// Select and persist the active scene when a config file is available.
    pub const SET_ACTIVE_SCENE: &str = "SetActiveScene";
    /// Return the capture-device config value used by the next recording.
    pub const GET_CAPTURE_DEVICE: &str = "GetCaptureDevice";
    /// Select and persist the capture device used by the next recording.
    pub const SET_CAPTURE_DEVICE: &str = "SetCaptureDevice";
    /// Return target/effective ASR state plus configured provider rows.
    pub const GET_ASR_MENU_STATE: &str = "GetAsrMenuState";
    /// Select, persist, and queue reload for a configured ASR provider.
    pub const SET_ACTIVE_ASR_PROVIDER: &str = "SetActiveAsrProvider";
    /// Return target/effective ASR state plus provider/model rows.
    pub const GET_ASR_TARGET_MENU_STATE: &str = "GetAsrTargetMenuState";
    /// Select, persist, and queue reload for an ASR provider/model target.
    pub const SET_ACTIVE_ASR_TARGET: &str = "SetActiveAsrTarget";
    /// Return target/effective ASR state plus localized provider/model rows.
    pub const GET_ASR_DISPLAY_MENU_STATE: &str = "GetAsrDisplayMenuState";
    /// Reload the selected ASR backend.
    pub const RELOAD_ASR_BACKEND: &str = "ReloadAsrBackend";
    /// Start a configured LLM adapter process.
    pub const START_ADAPTER: &str = "StartAdapter";
    /// Stop a configured LLM adapter process.
    pub const STOP_ADAPTER: &str = "StopAdapter";
    /// Frontend notifier method name on [`super::FRONTEND_NOTIFIER_INTERFACE`].
    pub const NOTIFY: &str = "Notify";
}

/// Stable status strings returned by `GetStatus` and `StatusChanged`.
pub mod status {
    /// The daemon is idle.
    pub const IDLE: &str = "idle";
    /// The daemon is recording audio.
    pub const RECORDING: &str = "recording";
    /// The daemon is running ASR inference.
    pub const INFERRING: &str = "inferring";
    /// The daemon is applying postprocessing.
    pub const POSTPROCESSING: &str = "postprocessing";
    /// The daemon entered an error state.
    pub const ERROR: &str = "error";
}

/// D-Bus error names that are part of the legacy ABI.
pub mod error {
    /// Legacy operation failure error returned by daemon methods.
    pub const OPERATION_FAILED: &str = "org.fcitx.Vinpst.Error.OperationFailed";
}

/// D-Bus signatures that are part of the legacy ABI.
pub mod signature {
    /// Legacy error-info tuple: `code`, `subject`, `detail`, `raw_message`.
    pub const ERROR_INFO: &str = "ssss";
}

/// Signal names on [`SERVICE_INTERFACE`].
pub mod signal {
    /// Final recognition payload. The first argument is a JSON string.
    pub const RECOGNITION_RESULT: &str = "RecognitionResult";
    /// Streaming partial text. The first argument is a string.
    pub const RECOGNITION_PARTIAL: &str = "RecognitionPartial";
    /// Daemon status transition. The first argument is a status string.
    pub const STATUS_CHANGED: &str = "StatusChanged";
    /// Daemon-originated notification payload with [`super::signature::ERROR_INFO`].
    pub const DAEMON_NOTIFICATION: &str = "DaemonNotification";
}

/// Legacy method names exported on [`SERVICE_INTERFACE`] in protocol order.
pub const LEGACY_SERVICE_METHODS: &[&str] = &[
    method::START_RECORDING,
    method::START_COMMAND_RECORDING,
    method::STOP_RECORDING,
    method::GET_STATUS,
    method::GET_ASR_BACKEND_STATE,
    method::RELOAD_ASR_BACKEND,
    method::START_ADAPTER,
    method::STOP_ADAPTER,
];

/// Rust-only diagnostic extension methods exported on [`SERVICE_INTERFACE`].
pub const DIAGNOSTIC_EXTENSION_METHODS: &[&str] =
    &[method::GET_TEXT_ADAPTER_STATE, method::GET_RUNTIME_STATUS];

/// Rust configuration extension methods exported on [`SERVICE_INTERFACE`].
pub const CONFIG_EXTENSION_METHODS: &[&str] = &[
    method::GET_SCENE_STATE,
    method::SET_ACTIVE_SCENE,
    method::GET_CAPTURE_DEVICE,
    method::SET_CAPTURE_DEVICE,
    method::GET_ASR_MENU_STATE,
    method::SET_ACTIVE_ASR_PROVIDER,
    method::GET_ASR_TARGET_MENU_STATE,
    method::SET_ACTIVE_ASR_TARGET,
    method::GET_ASR_DISPLAY_MENU_STATE,
];

/// Method names exported on [`SERVICE_INTERFACE`] in protocol order.
pub const SERVICE_METHODS: &[&str] = &[
    method::START_RECORDING,
    method::START_COMMAND_RECORDING,
    method::STOP_RECORDING,
    method::GET_STATUS,
    method::GET_ASR_BACKEND_STATE,
    method::GET_TEXT_ADAPTER_STATE,
    method::GET_RUNTIME_STATUS,
    method::GET_SCENE_STATE,
    method::SET_ACTIVE_SCENE,
    method::GET_CAPTURE_DEVICE,
    method::SET_CAPTURE_DEVICE,
    method::GET_ASR_MENU_STATE,
    method::SET_ACTIVE_ASR_PROVIDER,
    method::GET_ASR_TARGET_MENU_STATE,
    method::SET_ACTIVE_ASR_TARGET,
    method::GET_ASR_DISPLAY_MENU_STATE,
    method::RELOAD_ASR_BACKEND,
    method::START_ADAPTER,
    method::STOP_ADAPTER,
];

/// Signal names emitted on [`SERVICE_INTERFACE`] in protocol order.
pub const SERVICE_SIGNALS: &[&str] = &[
    signal::RECOGNITION_RESULT,
    signal::RECOGNITION_PARTIAL,
    signal::STATUS_CHANGED,
    signal::DAEMON_NOTIFICATION,
];
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbus_names_match_the_legacy_contract() {
        assert_eq!(FCITX_BUS_NAME, "org.fcitx.Fcitx5");
        assert_eq!(SERVICE_BUS_NAME, "org.fcitx.Vinpst");
        assert_eq!(SERVICE_OBJECT_PATH, "/org/fcitx/Vinpst");
        assert_eq!(SERVICE_INTERFACE, "org.fcitx.Vinpst.Service");
        assert_eq!(FRONTEND_NOTIFIER_OBJECT_PATH, "/org/fcitx/Fcitx5/Vinpst");
        assert_eq!(FRONTEND_NOTIFIER_INTERFACE, "org.fcitx.Fcitx5.Vinpst1");
        assert_eq!(method::START_RECORDING, "StartRecording");
        assert_eq!(method::GET_TEXT_ADAPTER_STATE, "GetTextAdapterState");
        assert_eq!(method::GET_RUNTIME_STATUS, "GetRuntimeStatus");
        assert_eq!(method::GET_SCENE_STATE, "GetSceneState");
        assert_eq!(method::SET_ACTIVE_SCENE, "SetActiveScene");
        assert_eq!(method::GET_ASR_MENU_STATE, "GetAsrMenuState");
        assert_eq!(method::SET_ACTIVE_ASR_PROVIDER, "SetActiveAsrProvider");
        assert_eq!(method::GET_ASR_TARGET_MENU_STATE, "GetAsrTargetMenuState");
        assert_eq!(method::SET_ACTIVE_ASR_TARGET, "SetActiveAsrTarget");
        assert_eq!(method::GET_ASR_DISPLAY_MENU_STATE, "GetAsrDisplayMenuState");
        assert_eq!(method::NOTIFY, "Notify");
        assert_eq!(status::IDLE, "idle");
        assert_eq!(status::RECORDING, "recording");
        assert_eq!(status::INFERRING, "inferring");
        assert_eq!(status::POSTPROCESSING, "postprocessing");
        assert_eq!(status::ERROR, "error");
        assert_eq!(
            error::OPERATION_FAILED,
            "org.fcitx.Vinpst.Error.OperationFailed"
        );
        assert_eq!(signature::ERROR_INFO, "ssss");
        assert_eq!(signal::RECOGNITION_RESULT, "RecognitionResult");
    }

    #[test]
    fn dbus_error_contract_matches_legacy_frontend_expectations() {
        assert_eq!(
            error::OPERATION_FAILED,
            "org.fcitx.Vinpst.Error.OperationFailed"
        );
        assert_eq!(signature::ERROR_INFO, "ssss");
        assert_eq!(method::NOTIFY, "Notify");
        assert_eq!(status::IDLE, "idle");
        assert_eq!(status::RECORDING, "recording");
        assert_eq!(status::INFERRING, "inferring");
        assert_eq!(status::POSTPROCESSING, "postprocessing");
        assert_eq!(status::ERROR, "error");
    }

    #[test]
    fn legacy_service_methods_exclude_diagnostic_extensions() {
        assert_eq!(
            LEGACY_SERVICE_METHODS,
            [
                method::START_RECORDING,
                method::START_COMMAND_RECORDING,
                method::STOP_RECORDING,
                method::GET_STATUS,
                method::GET_ASR_BACKEND_STATE,
                method::RELOAD_ASR_BACKEND,
                method::START_ADAPTER,
                method::STOP_ADAPTER,
            ]
        );
        assert_eq!(
            DIAGNOSTIC_EXTENSION_METHODS,
            [method::GET_TEXT_ADAPTER_STATE, method::GET_RUNTIME_STATUS]
        );
        assert!(!LEGACY_SERVICE_METHODS.contains(&method::GET_TEXT_ADAPTER_STATE));
        assert!(!LEGACY_SERVICE_METHODS.contains(&method::GET_RUNTIME_STATUS));
    }

    #[test]
    fn service_methods_do_not_include_frontend_notifier_methods() {
        assert!(!SERVICE_METHODS.contains(&method::NOTIFY));
        assert!(!LEGACY_SERVICE_METHODS.contains(&method::NOTIFY));
        assert!(!DIAGNOSTIC_EXTENSION_METHODS.contains(&method::NOTIFY));
        assert!(!CONFIG_EXTENSION_METHODS.contains(&method::NOTIFY));
        assert!(DIAGNOSTIC_EXTENSION_METHODS.contains(&method::GET_RUNTIME_STATUS));
        assert!(CONFIG_EXTENSION_METHODS.contains(&method::GET_SCENE_STATE));
        assert!(CONFIG_EXTENSION_METHODS.contains(&method::GET_ASR_MENU_STATE));
        assert!(CONFIG_EXTENSION_METHODS.contains(&method::GET_ASR_TARGET_MENU_STATE));
        assert!(CONFIG_EXTENSION_METHODS.contains(&method::GET_ASR_DISPLAY_MENU_STATE));
    }

    #[test]
    fn service_method_list_includes_legacy_methods_and_extensions() {
        let mut combined = LEGACY_SERVICE_METHODS.to_vec();
        combined.splice(5..5, DIAGNOSTIC_EXTENSION_METHODS.iter().copied());
        combined.splice(7..7, CONFIG_EXTENSION_METHODS.iter().copied());

        assert_eq!(SERVICE_METHODS, combined.as_slice());
    }

    #[test]
    fn dbus_member_lists_are_unique() {
        let method_count = SERVICE_METHODS
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let signal_count = SERVICE_SIGNALS
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();

        assert_eq!(SERVICE_METHODS.len(), method_count);
        assert_eq!(SERVICE_SIGNALS.len(), signal_count);
    }
}
