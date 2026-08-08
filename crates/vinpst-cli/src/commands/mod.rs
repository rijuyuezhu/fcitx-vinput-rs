mod adapter;
mod config;
mod device;
mod llm;
mod model;
mod provider;
mod registry;
mod scene;
mod system;

pub(crate) use adapter::handle_adapter_command;
pub(crate) use config::{
    ConfigEditRequest, ConfigSetRequest, handle_config_edit, handle_config_example,
    handle_config_get, handle_config_set, validate_config_file,
};
pub(crate) use device::handle_device_command;
pub(crate) use llm::handle_llm_command;
pub(crate) use model::handle_model_command;
pub(crate) use provider::{
    asr_provider_kind_label, handle_provider_command, normalize_provider_id,
};
pub(crate) use registry::{
    print_registry_install_plan, print_registry_plan, print_registry_summary,
    validate_registry_index,
};
pub(crate) use scene::handle_scene_command;
pub(crate) use system::{
    InitRequest, handle_init, print_asr_state, print_audio_devices, print_doctor, print_protocol,
    print_user_activation_service_status, remove_user_activation_service, validate_config,
    write_activation_service,
};
