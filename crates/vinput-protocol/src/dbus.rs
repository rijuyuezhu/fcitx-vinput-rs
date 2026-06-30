//! D-Bus names that must remain compatible with the C++ fcitx5-vinput addon.

/// Well-known bus name owned by Fcitx5.
pub const FCITX_BUS_NAME: &str = "org.fcitx.Fcitx5";

/// Well-known bus name owned by the Rust daemon.
pub const SERVICE_BUS_NAME: &str = "org.fcitx.Vinput";

/// Object path exported by the Rust daemon.
pub const SERVICE_OBJECT_PATH: &str = "/org/fcitx/Vinput";

/// Main service interface exported by the Rust daemon.
pub const SERVICE_INTERFACE: &str = "org.fcitx.Vinput.Service";

/// Object path used by the Fcitx5 addon-side notifier.
pub const FRONTEND_NOTIFIER_OBJECT_PATH: &str = "/org/fcitx/Fcitx5/Vinput";

/// Interface used by the Fcitx5 addon-side notifier.
pub const FRONTEND_NOTIFIER_INTERFACE: &str = "org.fcitx.Fcitx5.Vinput1";

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
    /// Return a JSON snapshot of the selected/effective ASR backend.
    pub const GET_ASR_BACKEND_STATE: &str = "GetAsrBackendState";
    /// Return a JSON snapshot of configured text adapters.
    pub const GET_TEXT_ADAPTER_STATE: &str = "GetTextAdapterState";
    /// Reload the selected ASR backend.
    pub const RELOAD_ASR_BACKEND: &str = "ReloadAsrBackend";
    /// Start a configured LLM adapter process.
    pub const START_ADAPTER: &str = "StartAdapter";
    /// Stop a configured LLM adapter process.
    pub const STOP_ADAPTER: &str = "StopAdapter";
    /// Frontend notifier method name on [`super::FRONTEND_NOTIFIER_INTERFACE`].
    pub const NOTIFY: &str = "Notify";
}

/// D-Bus error names that are part of the legacy ABI.
pub mod error {
    /// Legacy operation failure error returned by daemon methods.
    pub const OPERATION_FAILED: &str = "org.fcitx.Vinput.Error.OperationFailed";
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
pub const DIAGNOSTIC_EXTENSION_METHODS: &[&str] = &[method::GET_TEXT_ADAPTER_STATE];

/// Method names exported on [`SERVICE_INTERFACE`] in protocol order.
pub const SERVICE_METHODS: &[&str] = &[
    method::START_RECORDING,
    method::START_COMMAND_RECORDING,
    method::STOP_RECORDING,
    method::GET_STATUS,
    method::GET_ASR_BACKEND_STATE,
    method::GET_TEXT_ADAPTER_STATE,
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
        assert_eq!(SERVICE_BUS_NAME, "org.fcitx.Vinput");
        assert_eq!(SERVICE_OBJECT_PATH, "/org/fcitx/Vinput");
        assert_eq!(SERVICE_INTERFACE, "org.fcitx.Vinput.Service");
        assert_eq!(FRONTEND_NOTIFIER_OBJECT_PATH, "/org/fcitx/Fcitx5/Vinput");
        assert_eq!(FRONTEND_NOTIFIER_INTERFACE, "org.fcitx.Fcitx5.Vinput1");
        assert_eq!(method::START_RECORDING, "StartRecording");
        assert_eq!(method::GET_TEXT_ADAPTER_STATE, "GetTextAdapterState");
        assert_eq!(method::NOTIFY, "Notify");
        assert_eq!(
            error::OPERATION_FAILED,
            "org.fcitx.Vinput.Error.OperationFailed"
        );
        assert_eq!(signature::ERROR_INFO, "ssss");
        assert_eq!(signal::RECOGNITION_RESULT, "RecognitionResult");
    }

    #[test]
    fn dbus_error_contract_matches_legacy_frontend_expectations() {
        assert_eq!(
            error::OPERATION_FAILED,
            "org.fcitx.Vinput.Error.OperationFailed"
        );
        assert_eq!(signature::ERROR_INFO, "ssss");
        assert_eq!(method::NOTIFY, "Notify");
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
            [method::GET_TEXT_ADAPTER_STATE]
        );
        assert!(!LEGACY_SERVICE_METHODS.contains(&method::GET_TEXT_ADAPTER_STATE));
    }

    #[test]
    fn service_methods_do_not_include_frontend_notifier_methods() {
        assert!(!SERVICE_METHODS.contains(&method::NOTIFY));
        assert!(!LEGACY_SERVICE_METHODS.contains(&method::NOTIFY));
        assert!(!DIAGNOSTIC_EXTENSION_METHODS.contains(&method::NOTIFY));
    }

    #[test]
    fn service_method_list_includes_legacy_methods_and_extensions() {
        let mut combined = LEGACY_SERVICE_METHODS.to_vec();
        combined.splice(5..5, DIAGNOSTIC_EXTENSION_METHODS.iter().copied());

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
