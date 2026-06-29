//! Deterministic text finishing helpers and adapter seams.

mod adapter_runtime;
mod command;
mod context_cache;
mod core;
mod error;
mod openai;
mod payload;
mod prompt;

pub use adapter_runtime::{
    AdapterProcessSpec, AdapterRuntimePaths, AdapterStopOutcome, StartedAdapterProcess,
    default_adapter_runtime_dir, start_adapter_process, stop_adapter_process,
};
pub use command::{
    AdapterRegistry, CommandTextAdapter, CommandTextProcessor, CommandTextRequest,
    CommandTextResponse, CommandTextRunner, CommandTextScene, ProcessCommandTextRunner,
    UnsupportedCommandTextRunner,
};
pub use context_cache::{
    RecentInputContextEntry, append_recent_input_context_buffer, append_recent_input_context_entry,
    build_recent_input_context_prefix, default_context_cache_path,
    default_context_cache_path_for_current_user, load_recent_input_context_prefix,
    truncate_recent_input_context_cache,
};
pub(crate) use core::scene_needs_postprocessing;
pub use core::{
    LlmTextProcessor, MockTextProcessor, TextAdapter, TextFinisher, TextProcessor, TextRequest,
    UnsupportedTextAdapter,
};
pub use error::TextError;
pub use openai::{
    OpenAiCompatibleChatRequest, OpenAiCompatibleChatTransport, OpenAiCompatibleTextAdapter,
    OpenAiCompatibleTextProcessor, build_openai_compatible_chat_request,
    build_openai_compatible_chat_request_from_context_cache, build_openai_compatible_chat_url,
    build_openai_compatible_headers, extract_openai_compatible_candidates,
    merge_openai_compatible_extra_body, openai_compatible_candidates_to_payload,
    openai_compatible_response_to_payload,
};
pub use payload::command_mode_payload;
pub use prompt::{
    PromptContext, PromptTemplate, has_legacy_prompt_interpolation, is_prompt_file_uri,
    load_prompt_file_uri,
};

#[cfg(test)]
mod tests;
