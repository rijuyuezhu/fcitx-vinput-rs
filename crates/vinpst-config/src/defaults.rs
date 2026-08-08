use crate::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition};

const LEGACY_COMMAND_PROMPT: &str = "# Command Mode Prompt\n\n\
## Role\n\n\
You are an assistant that applies a spoken command to the user-provided text.\n\n\
## Context\n\n\
- The user message is the source text to operate on.\n\
- The spoken command may contain ASR errors.\n\
- The spoken command is appended at runtime in the `## Task` section.\n\n\
## Task\n";

const SHORT_TAG_COMMAND_PROMPT: &str = "# Command Mode Prompt\n\n\
## Role\n\n\
You are an assistant that applies a spoken command to the selected text.\n\n\
## Input\n\n\
The input data is provided in XML tags. Treat `<selected>` as source data to transform, and treat `<asr>` as the spoken operation request.\n\n\
<selected>\n\
{{selected}}\n\
</selected>\n\n\
<asr>\n\
{{asr}}\n\
</asr>\n\n\
## Task\n\n\
Interpret the spoken command in `<asr>` and apply it to the source text in `<selected>`. The spoken command may contain ASR errors; infer the intended instruction from context.\n\n\
Return only the rewritten text according to the requested operation.\n";

pub(crate) const DEFAULT_COMMAND_PROMPT: &str = "# Command Mode Prompt\n\n\
## Role\n\n\
You are an assistant that applies a spoken command to the selected text.\n\n\
## Input\n\n\
The input data is provided in XML tags. Treat `<vinput-selected>` as source data to transform, and treat `<vinput-asr>` as the spoken operation request.\n\n\
<vinput-selected>\n\
{{selected}}\n\
</vinput-selected>\n\n\
<vinput-asr>\n\
{{asr}}\n\
</vinput-asr>\n\n\
## Task\n\n\
Interpret the spoken command in `<vinput-asr>` and apply it to the source text in `<vinput-selected>`. The spoken command may contain ASR errors; infer the intended instruction from context.\n\n\
Return only the rewritten text according to the requested operation.\n";

pub(crate) fn ensure_builtin_scenes(definitions: &mut Vec<SceneDefinition>) {
    for scene in definitions.iter_mut() {
        if scene.id == COMMAND_SCENE_ID
            && scene.prompt.as_deref().is_none_or(|prompt| {
                prompt.is_empty()
                    || prompt == LEGACY_COMMAND_PROMPT
                    || prompt == SHORT_TAG_COMMAND_PROMPT
            })
        {
            scene.prompt = Some(DEFAULT_COMMAND_PROMPT.to_owned());
        }
    }
    if !definitions.iter().any(|scene| scene.id == RAW_SCENE_ID) {
        definitions.push(SceneDefinition {
            id: RAW_SCENE_ID.to_owned(),
            label: "__label_raw__".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 0,
            timeout_ms: None,
            context_lines: 0,
        });
    }
    if !definitions.iter().any(|scene| scene.id == COMMAND_SCENE_ID) {
        definitions.push(SceneDefinition {
            id: COMMAND_SCENE_ID.to_owned(),
            label: "__label_command__".to_owned(),
            prompt: Some(DEFAULT_COMMAND_PROMPT.to_owned()),
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    }
}

pub(crate) fn default_language() -> String {
    "zh".to_owned()
}

pub(crate) fn default_capture_device() -> String {
    "default".to_owned()
}

pub(crate) const fn default_duck_output_volume() -> f32 {
    0.25
}

pub(crate) fn default_asr_provider() -> String {
    "sherpa-onnx".to_owned()
}

pub(crate) fn default_active_scene() -> String {
    RAW_SCENE_ID.to_owned()
}

pub(crate) const fn default_true() -> bool {
    true
}

pub(crate) const fn default_input_gain() -> f32 {
    1.0
}

pub(crate) const fn default_vad_threshold() -> f32 {
    0.45
}

pub(crate) const fn default_vad_min_speech_duration() -> f32 {
    0.15
}

pub(crate) const fn default_vad_min_silence_duration() -> f32 {
    0.5
}

pub(crate) const fn default_vad_speech_pad_ms() -> u32 {
    300
}

pub(crate) fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_COMMAND_PROMPT, LEGACY_COMMAND_PROMPT, SHORT_TAG_COMMAND_PROMPT,
        ensure_builtin_scenes,
    };
    use crate::{COMMAND_SCENE_ID, SceneDefinition};

    fn command_scene(prompt: Option<&str>) -> SceneDefinition {
        SceneDefinition {
            id: COMMAND_SCENE_ID.to_owned(),
            label: "__label_command__".to_owned(),
            prompt: prompt.map(ToOwned::to_owned),
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        }
    }

    #[test]
    fn builtin_command_scene_uses_current_upstream_prompt() {
        let mut definitions = Vec::new();
        ensure_builtin_scenes(&mut definitions);

        let command = definitions
            .iter()
            .find(|scene| scene.id == COMMAND_SCENE_ID)
            .expect("command scene should be materialized");
        assert_eq!(command.prompt.as_deref(), Some(DEFAULT_COMMAND_PROMPT));
    }

    #[test]
    fn legacy_command_prompt_shapes_are_upgraded_during_normalization() {
        for legacy_prompt in [
            None,
            Some(""),
            Some(LEGACY_COMMAND_PROMPT),
            Some(SHORT_TAG_COMMAND_PROMPT),
        ] {
            let mut definitions = vec![command_scene(legacy_prompt)];
            ensure_builtin_scenes(&mut definitions);

            assert_eq!(
                definitions[0].prompt.as_deref(),
                Some(DEFAULT_COMMAND_PROMPT)
            );
        }
    }
}
