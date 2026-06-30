use super::{
    AdapterProcessSpec, AdapterRuntimePaths, AdapterStopOutcome, CommandTextAdapter,
    CommandTextProcessor, CommandTextRequest, CommandTextResponse, CommandTextRunner,
    LlmTextProcessor, MockTextProcessor, OpenAiCompatibleChatRequest,
    OpenAiCompatibleChatTransport, OpenAiCompatibleTextAdapter, OpenAiCompatibleTextProcessor,
    ProcessCommandTextRunner, PromptContext, PromptTemplate, RecentInputContextEntry,
    ReqwestOpenAiCompatibleChatTransport, TextAdapter, TextError, TextFinisher, TextProcessor,
    TextRequest, UnsupportedTextAdapter, append_recent_input_context_buffer,
    append_recent_input_context_entry, build_openai_compatible_chat_request,
    build_openai_compatible_chat_request_from_context_cache, build_openai_compatible_chat_url,
    build_openai_compatible_headers, build_recent_input_context_prefix, command_mode_payload,
    default_adapter_runtime_dir, default_context_cache_path, extract_openai_compatible_candidates,
    has_legacy_prompt_interpolation, is_prompt_file_uri, load_prompt_file_uri,
    load_recent_input_context_prefix, merge_openai_compatible_extra_body,
    openai_compatible_candidates_to_payload, openai_compatible_response_to_payload,
    start_adapter_process, stop_adapter_process, truncate_recent_input_context_cache,
};
use vinput_config::{
    COMMAND_SCENE_ID, LlmAdapterConfig, LlmProviderConfig, RAW_SCENE_ID, SceneDefinition,
};
use vinput_protocol::RecognitionPayload;

#[derive(Debug, Clone, Copy)]
struct EchoCommandRunner;

impl CommandTextRunner for EchoCommandRunner {
    fn run(
        &self,
        _adapter_id: &str,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError> {
        Ok(RecognitionPayload::raw(format!(
            "{} {} {} {}: {}",
            command,
            args.join(" "),
            env.get("MODE").map(String::as_str).unwrap_or_default(),
            working_dir.unwrap_or_default(),
            request.raw_text
        )))
    }
}

#[derive(Debug, Clone)]
struct StaticOpenAiTransport {
    response_body: String,
    seen_request: std::sync::Arc<std::sync::Mutex<Option<OpenAiCompatibleChatRequest>>>,
    seen_timeout_ms: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
}

impl StaticOpenAiTransport {
    fn new(response_body: String) -> Self {
        Self {
            response_body,
            seen_request: std::sync::Arc::new(std::sync::Mutex::new(None)),
            seen_timeout_ms: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl OpenAiCompatibleChatTransport for StaticOpenAiTransport {
    fn send(
        &self,
        request: &OpenAiCompatibleChatRequest,
        timeout_ms: Option<u64>,
    ) -> Result<String, TextError> {
        *self.seen_request.lock().unwrap() = Some(request.clone());
        *self.seen_timeout_ms.lock().unwrap() = timeout_ms;
        Ok(self.response_body.clone())
    }
}

fn scene(id: &str, candidate_count: u8) -> SceneDefinition {
    SceneDefinition {
        id: id.to_owned(),
        label: id.to_owned(),
        prompt: None,
        provider_id: None,
        model: None,
        candidate_count,
        timeout_ms: None,
        context_lines: 0,
    }
}

fn provider(extra_body: serde_json::Value) -> LlmProviderConfig {
    LlmProviderConfig {
        id: "openai-compatible".to_owned(),
        base_url: "http://localhost:8080/v1".to_owned(),
        api_key: String::new(),
        model: Some("provider-model".to_owned()),
        extra_body,
        extra: std::collections::HashMap::default(),
    }
}

fn provider_with_id(id: &str, base_url: &str) -> LlmProviderConfig {
    LlmProviderConfig {
        id: id.to_owned(),
        base_url: base_url.to_owned(),
        api_key: String::new(),
        model: Some(format!("{id}-model")),
        extra_body: serde_json::json!({}),
        extra: std::collections::HashMap::default(),
    }
}

#[derive(Debug)]
struct CapturedHttpRequest {
    head: String,
    body: String,
}

fn serve_single_http_response(
    status: &str,
    response_body: String,
) -> (String, std::thread::JoinHandle<CapturedHttpRequest>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let status = status.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
            assert_ne!(read, 0, "HTTP client closed before headers were complete");
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
        let headers = head
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect::<std::collections::HashMap<_, _>>();
        let body = if headers
            .get("transfer-encoding")
            .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
        {
            while !buffer[header_end..].windows(5).any(|window| {
                window
                    == b"0

"
            }) {
                let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
                assert_ne!(
                    read, 0,
                    "HTTP client closed before chunked body was complete"
                );
                buffer.extend_from_slice(&chunk[..read]);
            }
            decode_chunked_http_body(&buffer[header_end..])
        } else {
            let content_length = headers
                .get("content-length")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while buffer.len() < header_end + content_length {
                let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
                assert_ne!(read, 0, "HTTP client closed before body was complete");
                buffer.extend_from_slice(&chunk[..read]);
            }
            String::from_utf8_lossy(&buffer[header_end..header_end + content_length]).into_owned()
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        CapturedHttpRequest { head, body }
    });
    (base_url, handle)
}

fn decode_chunked_http_body(input: &[u8]) -> String {
    let mut position = 0;
    let mut decoded = Vec::new();
    while let Some(line_end) = input[position..]
        .windows(2)
        .position(|window| window == b"\r\n")
    {
        let line = String::from_utf8_lossy(&input[position..position + line_end]);
        let chunk_len = usize::from_str_radix(line.trim(), 16).unwrap();
        position += line_end + 2;
        if chunk_len == 0 {
            break;
        }
        decoded.extend_from_slice(&input[position..position + chunk_len]);
        position += chunk_len + 2;
    }
    String::from_utf8(decoded).unwrap()
}

fn serve_delayed_http_response(response_body: String, delay: std::time::Duration) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = std::io::Read::read(&mut stream, &mut chunk).unwrap_or(0);
            if read == 0 {
                return;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        std::thread::sleep(delay);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
    });
    base_url
}

#[test]
fn raw_scene_returns_raw_text() {
    let raw = scene(RAW_SCENE_ID, 0);
    let payload = TextFinisher::finish(&TextRequest {
        raw_text: "hello",
        scene: &raw,
        selected_text: None,
    })
    .unwrap();
    assert_eq!(payload.commit_text, "hello");
}

#[test]
fn prompt_context_exposes_scene_metadata() {
    let templated = SceneDefinition {
        prompt: Some("polish".to_owned()),
        provider_id: Some("p".to_owned()),
        model: Some("m".to_owned()),
        context_lines: 3,
        timeout_ms: Some(2500),
        ..scene("rewrite", 1)
    };
    let request = TextRequest {
        raw_text: "raw",
        scene: &templated,
        selected_text: Some("selected"),
    };

    let context = PromptContext::from_request(&request);
    assert_eq!(context.raw_text, "raw");
    assert_eq!(context.selected_text, "selected");
    assert_eq!(context.scene_id, "rewrite");
    assert_eq!(context.scene_prompt, "polish");
    assert_eq!(context.provider_id, "p");
    assert_eq!(context.model, "m");
    assert_eq!(context.candidate_count, 1);
    assert_eq!(context.context_lines, 3);
    assert_eq!(context.timeout_ms, Some(2500));
}

#[test]
fn prompt_template_replaces_supported_fields() {
    let templated = SceneDefinition {
        prompt: Some("polish".to_owned()),
        provider_id: Some("p".to_owned()),
        model: Some("m".to_owned()),
        context_lines: 3,
        timeout_ms: Some(2500),
        ..scene("rewrite", 1)
    };
    let request = TextRequest {
        raw_text: "raw",
        scene: &templated,
        selected_text: Some("selected"),
    };
    let context = PromptContext::from_request(&request);
    let rendered = PromptTemplate::new(
            "scene={scene_id}; prompt={scene_prompt}; raw={raw_text}; selected={selected_text}; provider={provider_id}; model={model}; candidates={candidate_count}; context={context_lines}; timeout={timeout_ms}",
        )
        .render(&context);
    let rendered_from_request = PromptTemplate::new(
            "scene={scene_id}; prompt={scene_prompt}; raw={raw_text}; selected={selected_text}; provider={provider_id}; model={model}; candidates={candidate_count}; context={context_lines}; timeout={timeout_ms}",
        )
        .render_request(&request);
    assert_eq!(rendered_from_request, rendered);
    assert_eq!(
        rendered,
        "scene=rewrite; prompt=polish; raw=raw; selected=selected; provider=p; model=m; candidates=1; context=3; timeout=2500"
    );
}

#[test]
fn prompt_template_supports_legacy_double_brace_placeholders() {
    let command = SceneDefinition {
        prompt: Some("apply command".to_owned()),
        ..scene("__command__", 1)
    };
    let request = TextRequest {
        raw_text: "make it shorter",
        scene: &command,
        selected_text: Some("This is the selected text."),
    };

    let rendered = PromptTemplate::new(
            "prompt={scene_prompt}; asr={{ asr }}; selected={{selected}}; context={{ context }}; unknown={{ future }}",
        )
        .render_request(&request);

    assert_eq!(
        rendered,
        "prompt=apply command; asr=make it shorter; selected=This is the selected text.; context=; unknown={{ future }}"
    );
}

#[test]
fn reqwest_openai_transport_posts_json_and_returns_body() {
    let response_body = serde_json::json!({
        "choices": [{"message": {"content": serde_json::json!({"candidates": ["via http"]}).to_string()}}]
    })
    .to_string();
    let (base_url, handle) = serve_single_http_response("200 OK", response_body);
    let request = OpenAiCompatibleChatRequest {
        url: format!("{base_url}/v1/chat/completions"),
        headers: vec![
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), "Bearer secret-token".to_owned()),
        ],
        body: serde_json::json!({
            "model": "model-id",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": false,
        }),
        ignored_extra_body_keys: Vec::new(),
    };

    let body = ReqwestOpenAiCompatibleChatTransport::new()
        .send(&request, Some(2_000))
        .unwrap();

    assert_eq!(extract_openai_compatible_candidates(&body), ["via http"]);
    let captured = handle.join().unwrap();
    assert!(
        captured
            .head
            .starts_with("POST /v1/chat/completions HTTP/1.1")
    );
    let lower_head = captured.head.to_ascii_lowercase();
    assert!(lower_head.contains("authorization: bearer secret-token"));
    assert!(lower_head.contains("content-type: application/json"));
    let posted: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
    assert_eq!(posted["model"], "model-id");
    assert_eq!(posted["messages"][0]["content"], "hello");
}

#[test]
fn reqwest_openai_transport_reports_http_errors_with_body() {
    let (base_url, handle) =
        serve_single_http_response("500 Internal Server Error", "boom".to_owned());
    let request = OpenAiCompatibleChatRequest {
        url: format!("{base_url}/chat/completions"),
        headers: build_openai_compatible_headers(""),
        body: serde_json::json!({"messages": []}),
        ignored_extra_body_keys: Vec::new(),
    };

    let error = ReqwestOpenAiCompatibleChatTransport::new()
        .send(&request, Some(2_000))
        .unwrap_err();

    handle.join().unwrap();
    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("HTTP 500") && message.contains("boom")
    ));
}

#[test]
fn reqwest_openai_transport_honors_request_timeout() {
    let response_body = serde_json::json!({
        "choices": [{"message": {"content": serde_json::json!({"candidates": ["late"]}).to_string()}}]
    })
    .to_string();
    let base_url =
        serve_delayed_http_response(response_body, std::time::Duration::from_millis(200));
    let request = OpenAiCompatibleChatRequest {
        url: format!("{base_url}/chat/completions"),
        headers: build_openai_compatible_headers(""),
        body: serde_json::json!({"messages": []}),
        ignored_extra_body_keys: Vec::new(),
    };
    let started = std::time::Instant::now();

    let error = ReqwestOpenAiCompatibleChatTransport::new()
        .send(&request, Some(25))
        .unwrap_err();

    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("HTTP request failed")
    ));
}

#[test]
fn openai_text_adapter_sends_request_and_maps_payload() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        model: Some("scene-model".to_owned()),
        timeout_ms: Some(2500),
        ..scene("polish", 0)
    };
    let mut provider = provider(serde_json::json!({}));
    provider.api_key = "secret-token".to_owned();
    let response_body = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({"candidates": ["polished"]}).to_string()
            }
        }]
    })
    .to_string();
    let transport = StaticOpenAiTransport::new(response_body);
    let seen_request = transport.seen_request.clone();
    let seen_timeout_ms = transport.seen_timeout_ms.clone();

    let payload = OpenAiCompatibleTextAdapter::new(provider, transport)
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap();

    assert_eq!(payload.commit_text, "polished");
    assert_eq!(payload.candidates[0].source.to_string(), "llm");
    let built = seen_request.lock().unwrap().clone().unwrap();
    assert_eq!(built.url, "http://localhost:8080/v1/chat/completions");
    assert_eq!(
        built.headers,
        [
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), "Bearer secret-token".to_owned()),
        ]
    );
    assert_eq!(built.body["messages"][0]["content"], "Polish: raw text");
    assert_eq!(*seen_timeout_ms.lock().unwrap(), Some(2500));
}

#[test]
fn openai_text_adapter_reports_response_without_candidates() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene("polish", 0)
    };
    let transport = StaticOpenAiTransport::new(serde_json::json!({"choices": []}).to_string());

    let error = OpenAiCompatibleTextAdapter::new(provider(serde_json::json!({})), transport)
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message) if message.contains("did not contain candidates")
    ));
}

#[test]
fn openai_text_processor_selects_scene_provider_id() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        provider_id: Some("second".to_owned()),
        ..scene("polish", 0)
    };
    let transport = StaticOpenAiTransport::new(
            serde_json::json!({
                "choices": [{"message": {"content": serde_json::json!({"candidates": ["polished"]}).to_string()}}]
            })
            .to_string(),
        );
    let seen_request = transport.seen_request.clone();

    let payload = OpenAiCompatibleTextProcessor::new(
        vec![
            provider_with_id("first", "https://first.example/v1"),
            provider_with_id("second", "https://second.example/v1"),
        ],
        transport,
    )
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "polished");
    let built = seen_request.lock().unwrap().clone().unwrap();
    assert_eq!(built.url, "https://second.example/v1/chat/completions");
    assert_eq!(built.body["model"], "second-model");
}

#[test]
fn openai_text_processor_uses_single_provider_without_scene_provider_id() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        ..scene("polish", 0)
    };
    let transport = StaticOpenAiTransport::new(
            serde_json::json!({
                "choices": [{"message": {"content": serde_json::json!({"candidates": ["polished"]}).to_string()}}]
            })
            .to_string(),
        );
    let seen_request = transport.seen_request.clone();

    let payload = OpenAiCompatibleTextProcessor::new(
        vec![provider_with_id("single", "https://single.example/v1")],
        transport,
    )
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "polished");
    let built = seen_request.lock().unwrap().clone().unwrap();
    assert_eq!(built.url, "https://single.example/v1/chat/completions");
}

#[test]
fn openai_text_processor_requires_provider_for_prompted_scene() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        ..scene("polish", 0)
    };

    let error = OpenAiCompatibleTextProcessor::new(
        Vec::new(),
        StaticOpenAiTransport::new(serde_json::json!({"choices": []}).to_string()),
    )
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert_eq!(error, TextError::AdapterRequired("polish".to_owned()));
}

#[test]
fn openai_text_processor_rejects_ambiguous_providers_without_scene_provider_id() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        ..scene("polish", 0)
    };

    let error = OpenAiCompatibleTextProcessor::new(
        vec![
            provider_with_id("first", "https://first.example/v1"),
            provider_with_id("second", "https://second.example/v1"),
        ],
        StaticOpenAiTransport::new(serde_json::json!({"choices": []}).to_string()),
    )
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert_eq!(error, TextError::AmbiguousProvider("polish".to_owned()));
}

#[test]
fn openai_text_processor_reports_unknown_scene_provider_id() {
    let prompted = SceneDefinition {
        prompt: Some("Polish: {{ asr }}".to_owned()),
        provider_id: Some("missing".to_owned()),
        ..scene("polish", 0)
    };

    let error = OpenAiCompatibleTextProcessor::new(
        vec![provider_with_id("default", "https://default.example/v1")],
        StaticOpenAiTransport::new(serde_json::json!({"choices": []}).to_string()),
    )
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert_eq!(
        error,
        TextError::UnknownProvider {
            scene_id: "polish".to_owned(),
            provider_id: "missing".to_owned(),
        }
    );
}

#[test]
fn openai_text_processor_uses_context_cache_path() {
    let tempdir = tempfile::tempdir().unwrap();
    let cache_path = tempdir.path().join("context.jsonl");
    std::fs::write(&cache_path, "older\nlatest\n").unwrap();
    let prompted = SceneDefinition {
        prompt: Some("Context={{ context }} ASR={{ asr }}".to_owned()),
        context_lines: 1,
        ..scene("polish", 0)
    };
    let transport = StaticOpenAiTransport::new(
            serde_json::json!({
                "choices": [{"message": {"content": serde_json::json!({"candidates": ["polished"]}).to_string()}}]
            })
            .to_string(),
        );
    let seen_request = transport.seen_request.clone();

    let payload = OpenAiCompatibleTextProcessor::new(
        vec![provider_with_id("single", "https://single.example/v1")],
        transport,
    )
    .with_context_cache_path(&cache_path)
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "polished");
    let built = seen_request.lock().unwrap().clone().unwrap();
    assert_eq!(
        built.body["messages"][0]["content"],
        "Context=Recent input history (use to fix ASR errors):\nlatest\n\n ASR=raw text"
    );
}

#[test]
fn openai_text_adapter_command_scene_orders_raw_asr_and_llm_candidates() {
    let command = SceneDefinition {
        prompt: Some("Rewrite selected text using command: {{ asr }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene(COMMAND_SCENE_ID, 0)
    };
    let response_body = serde_json::json!({
            "choices": [{
                "message": {
                    "content": serde_json::json!({"candidates": ["short rewrite", "second rewrite"]}).to_string()
                }
            }]
        })
        .to_string();

    let payload = OpenAiCompatibleTextAdapter::new(
        provider(serde_json::json!({})),
        StaticOpenAiTransport::new(response_body),
    )
    .finish(&TextRequest {
        raw_text: "make it shorter",
        scene: &command,
        selected_text: Some("This is the selected text."),
    })
    .unwrap();

    assert_eq!(payload.commit_text, "short rewrite");
    assert_eq!(payload.candidates.len(), 4);
    assert_eq!(payload.candidates[0].text, "This is the selected text.");
    assert_eq!(payload.candidates[0].source.to_string(), "raw");
    assert_eq!(payload.candidates[1].text, "make it shorter");
    assert_eq!(payload.candidates[1].source.to_string(), "asr");
    assert_eq!(payload.candidates[2].text, "short rewrite");
    assert_eq!(payload.candidates[2].source.to_string(), "llm");
    assert_eq!(payload.candidates[3].text, "second rewrite");
    assert_eq!(payload.candidates[3].source.to_string(), "llm");
}

#[test]
fn openai_text_adapter_command_scene_falls_back_to_selected_without_llm_candidates() {
    let command = SceneDefinition {
        prompt: Some("Rewrite selected text using command: {{ asr }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene(COMMAND_SCENE_ID, 0)
    };

    let payload = OpenAiCompatibleTextAdapter::new(
        provider(serde_json::json!({})),
        StaticOpenAiTransport::new(serde_json::json!({"choices": []}).to_string()),
    )
    .finish(&TextRequest {
        raw_text: "make it shorter",
        scene: &command,
        selected_text: Some("This is the selected text."),
    })
    .unwrap();

    assert_eq!(payload.commit_text, "This is the selected text.");
    assert_eq!(payload.candidates.len(), 2);
    assert_eq!(payload.candidates[0].text, "This is the selected text.");
    assert_eq!(payload.candidates[0].source.to_string(), "raw");
    assert_eq!(payload.candidates[1].text, "make it shorter");
    assert_eq!(payload.candidates[1].source.to_string(), "asr");
}

#[test]
fn openai_chat_request_wraps_xml_without_interpolation() {
    let prompted = SceneDefinition {
        prompt: Some("Polish this.".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        model: Some("scene-model".to_owned()),
        candidate_count: 2,
        ..scene("polish", 2)
    };
    let provider = provider(serde_json::json!({
        "top_p": 0.8,
        "messages": [{"role": "system", "content": "override"}],
    }));

    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "raw dictated",
            scene: &prompted,
            selected_text: None,
        },
        &provider,
        "previous line",
    )
    .unwrap()
    .unwrap();

    assert_eq!(built.url, "http://localhost:8080/v1/chat/completions");
    assert_eq!(
        built.headers,
        [("Content-Type".to_owned(), "application/json".to_owned())]
    );
    assert_eq!(built.ignored_extra_body_keys, ["messages"]);
    assert_eq!(built.body["model"], "scene-model");
    assert_eq!(built.body["stream"], false);
    assert_eq!(built.body["temperature"], 0.2);
    assert_eq!(
        built.body["response_format"],
        serde_json::json!({"type": "json_object"})
    );
    assert_eq!(built.body["top_p"], 0.8);
    let content = built.body["messages"][0]["content"].as_str().unwrap();
    assert!(content.starts_with(
        "Polish this.\n\n<context>\nprevious line\n</context>\n<asr>\nraw dictated\n</asr>\n"
    ));
    assert!(content.contains("\n\n## Constraints\n"));
    assert!(content.contains("Return EXACTLY 2 candidate(s)"));
    assert!(content.contains("{\"candidates\": [\"<string>\", \"<string>\"]}"));
}

#[test]
fn openai_chat_request_wraps_selected_xml_for_command_scene() {
    let command = SceneDefinition {
        prompt: Some("Apply the command.".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene(COMMAND_SCENE_ID, 0)
    };

    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "make it shorter",
            scene: &command,
            selected_text: Some("This is the selected text."),
        },
        &provider(serde_json::json!({})),
        "",
    )
    .unwrap()
    .unwrap();

    let content = built.body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(
        content,
        "Apply the command.\n\n<asr>\nmake it shorter\n</asr>\n<selected>\nThis is the selected text.\n</selected>\n"
    );
}

#[test]
fn openai_chat_request_interpolates_context_and_selected_without_xml() {
    let prompted = SceneDefinition {
        prompt: Some("Context={{ context }} ASR={{ asr }} Selected={{ selected }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene("polish", 0)
    };
    let provider = provider(serde_json::json!({
        "stream": true,
        "response_format": {"type": "text"},
        "frequency_penalty": 0.5,
    }));

    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "fix text",
            scene: &prompted,
            selected_text: Some("source text"),
        },
        &provider,
        "recent input\n",
    )
    .unwrap()
    .unwrap();

    assert_eq!(built.ignored_extra_body_keys.len(), 2);
    assert!(
        built
            .ignored_extra_body_keys
            .iter()
            .any(|key| key == "stream")
    );
    assert!(
        built
            .ignored_extra_body_keys
            .iter()
            .any(|key| key == "response_format")
    );
    assert_eq!(built.url, "http://localhost:8080/v1/chat/completions");
    assert_eq!(
        built.headers,
        [("Content-Type".to_owned(), "application/json".to_owned())]
    );
    assert_eq!(built.body["model"], "provider-model");
    assert_eq!(built.body["stream"], false);
    assert_eq!(
        built.body["response_format"],
        serde_json::json!({"type": "json_object"})
    );
    assert_eq!(built.body["frequency_penalty"], 0.5);
    let content = built.body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(
        content,
        "Context=recent input\n ASR=fix text Selected=source text"
    );
    assert!(!content.contains("<asr>"));
    assert!(!content.contains("## Constraints"));
}

#[test]
fn openai_chat_request_from_context_cache_uses_scene_context_lines() {
    let tempdir = tempfile::tempdir().unwrap();
    let cache_path = tempdir.path().join("context.jsonl");
    std::fs::write(
        &cache_path,
        "older
latest
",
    )
    .unwrap();
    let prompted = SceneDefinition {
        prompt: Some("Context={{ context }} ASR={{ asr }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        context_lines: 1,
        ..scene("polish", 0)
    };

    let built = build_openai_compatible_chat_request_from_context_cache(
        &TextRequest {
            raw_text: "fix text",
            scene: &prompted,
            selected_text: None,
        },
        &provider(serde_json::json!({})),
        &cache_path,
    )
    .unwrap()
    .unwrap();

    let content = built.body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(
        content,
        "Context=Recent input history (use to fix ASR errors):
latest

 ASR=fix text"
    );
}

#[test]
fn openai_chat_request_from_context_cache_ignores_missing_cache() {
    let tempdir = tempfile::tempdir().unwrap();
    let prompted = SceneDefinition {
        prompt: Some("Context={{ context }} ASR={{ asr }}".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        context_lines: 3,
        ..scene("polish", 0)
    };

    let built = build_openai_compatible_chat_request_from_context_cache(
        &TextRequest {
            raw_text: "fix text",
            scene: &prompted,
            selected_text: None,
        },
        &provider(serde_json::json!({})),
        tempdir.path().join("missing-context.jsonl"),
    )
    .unwrap()
    .unwrap();

    let content = built.body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(content, "Context= ASR=fix text");
}

#[test]
fn openai_chat_request_without_prompt_is_not_applicable() {
    let raw = scene("noop", 0);
    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "raw",
            scene: &raw,
            selected_text: None,
        },
        &provider(serde_json::json!({})),
        "",
    )
    .unwrap();

    assert!(built.is_none());
}

#[test]
fn openai_chat_request_without_base_url_is_not_applicable() {
    let prompted = SceneDefinition {
        prompt: Some("Polish this.".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene("polish", 0)
    };
    let mut provider = provider(serde_json::json!({}));
    provider.base_url.clear();

    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "raw",
            scene: &prompted,
            selected_text: None,
        },
        &provider,
        "",
    )
    .unwrap();

    assert!(built.is_none());
}

#[test]
fn openai_headers_include_json_content_type_and_optional_bearer() {
    assert_eq!(
        build_openai_compatible_headers(""),
        [("Content-Type".to_owned(), "application/json".to_owned())]
    );
    assert_eq!(
        build_openai_compatible_headers("secret-token"),
        [
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), "Bearer secret-token".to_owned()),
        ]
    );
}

#[test]
fn openai_chat_request_includes_bearer_header_when_api_key_is_set() {
    let prompted = SceneDefinition {
        prompt: Some("Polish this.".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene("polish", 0)
    };
    let mut provider = provider(serde_json::json!({}));
    provider.api_key = "secret-token".to_owned();

    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "raw",
            scene: &prompted,
            selected_text: None,
        },
        &provider,
        "",
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        built.headers,
        [
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), "Bearer secret-token".to_owned()),
        ]
    );
}

#[test]
fn openai_chat_request_debug_redacts_authorization_header() {
    let prompted = SceneDefinition {
        prompt: Some("Polish this.".to_owned()),
        provider_id: Some("openai-compatible".to_owned()),
        ..scene("polish", 0)
    };
    let mut provider = provider(serde_json::json!({}));
    provider.api_key = "secret-token".to_owned();

    let built = build_openai_compatible_chat_request(
        &TextRequest {
            raw_text: "raw",
            scene: &prompted,
            selected_text: None,
        },
        &provider,
        "",
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        built.redacted_headers(),
        [
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), "<redacted>".to_owned()),
        ]
    );
    let debug = format!("{built:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("secret-token"));
    assert!(
        built
            .headers
            .iter()
            .any(|(_, value)| value.contains("secret-token"))
    );
}

#[test]
fn openai_chat_request_redacts_authorization_header_case_insensitively() {
    let request = OpenAiCompatibleChatRequest {
        url: "https://api.example.test/v1/chat/completions".to_owned(),
        headers: vec![
            ("authorization".to_owned(), "Bearer lower-secret".to_owned()),
            ("AUTHORIZATION".to_owned(), "Bearer upper-secret".to_owned()),
            ("X-Trace".to_owned(), "trace-id".to_owned()),
        ],
        body: serde_json::json!({"model":"model-id"}),
        ignored_extra_body_keys: Vec::new(),
    };

    assert_eq!(
        request.redacted_headers(),
        [
            ("authorization".to_owned(), "<redacted>".to_owned()),
            ("AUTHORIZATION".to_owned(), "<redacted>".to_owned()),
            ("X-Trace".to_owned(), "trace-id".to_owned()),
        ]
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(_, value)| value.contains("lower-secret"))
    );
    let debug = format!("{request:?}");
    assert!(!debug.contains("lower-secret"));
    assert!(!debug.contains("upper-secret"));
    assert!(debug.contains("trace-id"));
}

#[test]
fn openai_chat_url_appends_chat_completions_path() {
    assert_eq!(
        build_openai_compatible_chat_url("https://api.example.test/v1").as_deref(),
        Some("https://api.example.test/v1/chat/completions")
    );
    assert_eq!(
        build_openai_compatible_chat_url("https://api.example.test/v1///").as_deref(),
        Some("https://api.example.test/v1/chat/completions")
    );
}

#[test]
fn openai_chat_url_preserves_complete_endpoint_and_rejects_empty_base() {
    assert_eq!(
        build_openai_compatible_chat_url("https://api.example.test/v1/chat/completions").as_deref(),
        Some("https://api.example.test/v1/chat/completions")
    );
    assert_eq!(build_openai_compatible_chat_url(""), None);
}

#[test]
fn recent_input_context_prefix_takes_last_non_empty_lines() {
    let prefix = build_recent_input_context_prefix(["first", "", "second", "third", "   "], 3);

    assert_eq!(
        prefix,
        "Recent input history (use to fix ASR errors):\nsecond\nthird\n   \n\n"
    );
}

#[test]
fn recent_input_context_prefix_returns_empty_for_zero_or_empty_input() {
    assert_eq!(build_recent_input_context_prefix(["first"], 0), "");
    assert_eq!(build_recent_input_context_prefix(["", ""], 2), "");
}

#[test]
fn recent_input_context_prefix_reads_cache_file() {
    let tempdir = tempfile::tempdir().unwrap();
    let cache_path = tempdir.path().join("context.jsonl");
    std::fs::write(&cache_path, "one\n\ntwo\nthree\n").unwrap();

    let prefix = load_recent_input_context_prefix(&cache_path, 2).unwrap();

    assert_eq!(
        prefix,
        "Recent input history (use to fix ASR errors):\ntwo\nthree\n\n"
    );
}

#[test]
fn recent_input_context_prefix_missing_cache_is_empty() {
    let tempdir = tempfile::tempdir().unwrap();
    let cache_path = tempdir.path().join("missing-context.jsonl");

    assert_eq!(
        load_recent_input_context_prefix(&cache_path, 3).unwrap(),
        ""
    );
}

#[test]
fn recent_input_context_buffer_joins_latin_with_spaces() {
    let mut buffer = String::new();

    assert!(!append_recent_input_context_buffer(&mut buffer, "hello"));
    assert!(!append_recent_input_context_buffer(&mut buffer, "world"));
    assert_eq!(buffer, "hello world");
    assert!(append_recent_input_context_buffer(&mut buffer, "done."));
    assert_eq!(buffer, "hello world done.");
}

#[test]
fn recent_input_context_buffer_keeps_cjk_boundaries_tight() {
    let mut buffer = String::new();

    assert!(!append_recent_input_context_buffer(&mut buffer, "你好"));
    assert!(append_recent_input_context_buffer(&mut buffer, "世界。"));
    assert_eq!(buffer, "你好世界。");
}

#[test]
fn recent_input_context_buffer_recognizes_legacy_sentence_enders() {
    for ender in [".", "!", "?", "。", "！", "？", "…", "\n"] {
        let mut buffer = String::from("text");
        assert!(
            append_recent_input_context_buffer(&mut buffer, ender),
            "expected `{ender}` to flush"
        );
    }
    let mut buffer = String::from("text");
    assert!(!append_recent_input_context_buffer(&mut buffer, "fragment"));
}

#[test]
fn recent_input_context_cache_appends_legacy_json_lines() {
    let tempdir = tempfile::tempdir().unwrap();
    let cache_path = tempdir.path().join("nested").join("context.jsonl");

    assert!(!append_recent_input_context_entry(&cache_path, "", "user", 1).unwrap());
    assert!(append_recent_input_context_entry(&cache_path, "hello", "", 123).unwrap());
    assert!(append_recent_input_context_entry(&cache_path, "world", "asr", 124).unwrap());

    let lines = std::fs::read_to_string(&cache_path).unwrap();
    let entries = lines
        .lines()
        .map(|line| serde_json::from_str::<RecentInputContextEntry>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [
            RecentInputContextEntry {
                text: "hello".to_owned(),
                source: "user".to_owned(),
                timestamp: 123,
            },
            RecentInputContextEntry {
                text: "world".to_owned(),
                source: "asr".to_owned(),
                timestamp: 124,
            },
        ]
    );

    let prefix = load_recent_input_context_prefix(&cache_path, 1).unwrap();
    assert_eq!(
        prefix,
        format!(
            "Recent input history (use to fix ASR errors):\n{}\n\n",
            serde_json::to_string(entries.last().unwrap()).unwrap()
        )
    );
}

#[test]
fn recent_input_context_cache_truncates_to_last_non_empty_lines() {
    let tempdir = tempfile::tempdir().unwrap();
    let cache_path = tempdir.path().join("context.jsonl");
    std::fs::write(&cache_path, "one\n\ntwo\nthree\nfour\n").unwrap();

    truncate_recent_input_context_cache(&cache_path, 2).unwrap();

    assert_eq!(
        std::fs::read_to_string(&cache_path).unwrap(),
        "three\nfour\n"
    );
    truncate_recent_input_context_cache(tempdir.path().join("missing.jsonl"), 2).unwrap();
}

#[test]
fn prompt_file_uri_loader_reads_absolute_file_uri() {
    let tempdir = tempfile::tempdir().unwrap();
    let prompt_path = tempdir.path().join("prompt.txt");
    std::fs::write(&prompt_path, "Please rewrite: {{ asr }}").unwrap();
    let uri = format!("file://{}", prompt_path.display());

    assert!(is_prompt_file_uri(&uri));
    assert_eq!(
        load_prompt_file_uri(&uri).unwrap(),
        "Please rewrite: {{ asr }}"
    );
}

#[test]
fn prompt_file_uri_loader_rejects_non_file_uri() {
    assert!(!is_prompt_file_uri("file://relative/prompt.txt"));
    assert_eq!(
        load_prompt_file_uri("file://relative/prompt.txt").unwrap_err(),
        TextError::PromptFileLoad("not a file:/// URI".to_owned())
    );
}

#[test]
fn prompt_file_uri_loader_rejects_empty_path() {
    assert_eq!(
        load_prompt_file_uri("file:///").unwrap_err(),
        TextError::PromptFileLoad("empty path".to_owned())
    );
}

#[test]
fn prompt_file_uri_loader_requires_regular_file() {
    let tempdir = tempfile::tempdir().unwrap();
    let uri = format!("file://{}", tempdir.path().display());

    assert_eq!(
        load_prompt_file_uri(&uri).unwrap_err(),
        TextError::PromptFileLoad("not a regular file".to_owned())
    );
}

#[test]
fn prompt_file_uri_loader_truncates_to_legacy_cap() {
    let tempdir = tempfile::tempdir().unwrap();
    let prompt_path = tempdir.path().join("prompt.txt");
    std::fs::write(&prompt_path, vec![b'a'; 256 * 1024 + 1]).unwrap();
    let uri = format!("file://{}", prompt_path.display());

    let prompt = load_prompt_file_uri(&uri).unwrap();

    assert_eq!(prompt.len(), 256 * 1024);
    assert!(prompt.bytes().all(|byte| byte == b'a'));
}

#[test]
fn legacy_prompt_interpolation_detection_matches_prefix_check() {
    assert!(has_legacy_prompt_interpolation("hello {{ asr }}"));
    assert!(has_legacy_prompt_interpolation("literal {{"));
    assert!(!has_legacy_prompt_interpolation("hello {raw_text}"));
}

#[test]
fn prompt_template_renders_missing_timeout_as_empty() {
    let raw = scene("raw", 0);
    let request = TextRequest {
        raw_text: "raw",
        scene: &raw,
        selected_text: None,
    };

    let rendered = PromptTemplate::new("timeout={timeout_ms}").render_request(&request);
    assert_eq!(rendered, "timeout=");
}

#[test]
fn prompt_template_renders_missing_selected_text_as_empty() {
    let raw = scene("raw", 0);
    let request = TextRequest {
        raw_text: "dictated text",
        scene: &raw,
        selected_text: None,
    };

    let rendered = PromptTemplate::new("selected={selected_text}; legacy={{selected}}")
        .render_request(&request);

    assert_eq!(rendered, "selected=; legacy=");
}

#[test]
fn prompt_template_keeps_unknown_placeholders() {
    let raw = scene("raw", 0);
    let request = TextRequest {
        raw_text: "raw",
        scene: &raw,
        selected_text: None,
    };

    let rendered = PromptTemplate::new("x={x}").render_request(&request);
    assert_eq!(rendered, "x={x}");
}

#[test]
fn default_adapter_runtime_dir_prefers_xdg_runtime_dir() {
    assert_eq!(
        default_adapter_runtime_dir(Some(std::path::Path::new("/run/user/1000"))),
        std::path::PathBuf::from("/run/user/1000/vinput/adapters")
    );
}

#[test]
fn default_adapter_runtime_dir_falls_back_to_temp_dir() {
    assert_eq!(
        default_adapter_runtime_dir(None),
        std::env::temp_dir().join("vinput").join("adapters")
    );
    assert_eq!(
        default_adapter_runtime_dir(Some(std::path::Path::new(""))),
        std::env::temp_dir().join("vinput").join("adapters")
    );
}

#[test]
fn default_context_cache_path_matches_legacy_xdg_order() {
    assert_eq!(
        default_context_cache_path(
            Some(std::path::Path::new("/cache-home")),
            Some(std::path::Path::new("/home/demo")),
        ),
        std::path::PathBuf::from("/cache-home/vinput/context.jsonl")
    );
    assert_eq!(
        default_context_cache_path(None, Some(std::path::Path::new("/home/demo"))),
        std::path::PathBuf::from("/home/demo/.cache/vinput/context.jsonl")
    );
    assert_eq!(
        default_context_cache_path(
            Some(std::path::Path::new("")),
            Some(std::path::Path::new("")),
        ),
        std::path::PathBuf::from("vinput/context.jsonl")
    );
}

#[test]
fn adapter_runtime_paths_build_safe_pid_paths() {
    let paths = AdapterRuntimePaths::new("/tmp/vinput-runtime");

    assert_eq!(
        paths.pid_path("adapter.demo").unwrap(),
        std::path::PathBuf::from("/tmp/vinput-runtime/adapter.demo.pid")
    );
    assert_eq!(
        paths.runtime_dir(),
        std::path::Path::new("/tmp/vinput-runtime")
    );
}

#[test]
fn adapter_runtime_paths_roundtrip_pid_files() {
    let mut runtime_dir = std::env::temp_dir();
    runtime_dir.push(format!(
        "vinput-text-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let paths = AdapterRuntimePaths::new(&runtime_dir);

    let pid_path = paths.write_pid("adapter.demo", 12345).unwrap();
    assert_eq!(pid_path, runtime_dir.join("adapter.demo.pid"));
    assert_eq!(paths.read_pid("adapter.demo").unwrap(), Some(12345));
    assert!(paths.remove_pid("adapter.demo").unwrap());
    assert_eq!(paths.read_pid("adapter.demo").unwrap(), None);
    assert!(!paths.remove_pid("adapter.demo").unwrap());
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn adapter_runtime_paths_reject_malformed_pid_files() {
    let mut runtime_dir = std::env::temp_dir();
    runtime_dir.push(format!(
        "vinput-text-runtime-bad-pid-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::write(runtime_dir.join("adapter.demo.pid"), "not-a-pid").unwrap();
    let paths = AdapterRuntimePaths::new(&runtime_dir);

    let error = paths.read_pid("adapter.demo").unwrap_err();
    assert!(
        matches!(error, TextError::InvalidAdapterPid(message) if message.contains("not-a-pid") || message.contains("invalid digit"))
    );
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn adapter_runtime_paths_reject_unsafe_adapter_ids() {
    let paths = AdapterRuntimePaths::new("/tmp/vinput-runtime");

    for adapter_id in ["", ".", "..", "../escape", "nested/id", r"nested\id"] {
        let error = paths.pid_path(adapter_id).unwrap_err();
        assert_eq!(error, TextError::InvalidAdapterId(adapter_id.to_owned()));
    }
}

#[test]
fn adapter_process_spec_copies_typed_config() {
    let spec = AdapterProcessSpec::from_config(&LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "helper".to_owned(),
        args: vec!["--serve".to_owned()],
        env: std::collections::HashMap::from([("MODE".to_owned(), "serve".to_owned())]),
        working_dir: Some("/tmp/vinput-adapter".to_owned()),
        extra: std::collections::HashMap::default(),
    });

    assert_eq!(spec.id, "cmd-adapter");
    assert_eq!(spec.command, "helper");
    assert_eq!(spec.args, ["--serve"]);
    assert_eq!(spec.env.get("MODE").map(String::as_str), Some("serve"));
    assert_eq!(spec.working_dir.as_deref(), Some("/tmp/vinput-adapter"));
}

#[test]
fn start_adapter_process_writes_pid_file_and_returns_child() {
    let mut runtime_dir = std::env::temp_dir();
    runtime_dir.push(format!(
        "vinput-text-process-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = AdapterProcessSpec {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "sleep 30".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
    };

    let mut started = start_adapter_process(&spec, &paths).unwrap();
    assert_eq!(started.id, "cmd-adapter");
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), Some(started.pid));
    assert_eq!(started.pid_path, runtime_dir.join("cmd-adapter.pid"));
    started.child.kill().unwrap();
    let _ = started.child.wait();
    assert!(paths.remove_pid("cmd-adapter").unwrap());
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn start_adapter_process_reports_spawn_failure_without_pid_file() {
    let mut runtime_dir = std::env::temp_dir();
    runtime_dir.push(format!(
        "vinput-text-process-runtime-missing-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = AdapterProcessSpec {
        id: "cmd-adapter".to_owned(),
        command: format!("vinput-missing-adapter-{}", std::process::id()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
    };

    let error = start_adapter_process(&spec, &paths).unwrap_err();
    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("failed to spawn text adapter `cmd-adapter`")
    ));
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
}

#[test]
fn stop_adapter_process_terminates_child_and_removes_pid_file() {
    let mut runtime_dir = std::env::temp_dir();
    runtime_dir.push(format!(
        "vinput-text-stop-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let paths = AdapterRuntimePaths::new(&runtime_dir);
    let spec = AdapterProcessSpec {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "sleep 30".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
    };
    let mut started = start_adapter_process(&spec, &paths).unwrap();

    let outcome = stop_adapter_process("cmd-adapter", &paths).unwrap();
    assert_eq!(outcome, AdapterStopOutcome::Stopped { pid: started.pid });
    let _ = started.child.wait();
    assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    std::fs::remove_dir_all(runtime_dir).unwrap();
}

#[test]
fn stop_adapter_process_reports_not_running_without_pid_file() {
    let mut runtime_dir = std::env::temp_dir();
    runtime_dir.push(format!(
        "vinput-text-stop-runtime-empty-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let paths = AdapterRuntimePaths::new(&runtime_dir);

    assert_eq!(
        stop_adapter_process("cmd-adapter", &paths).unwrap(),
        AdapterStopOutcome::NotRunning
    );
}

#[test]
fn command_mode_payload_orders_raw_asr_and_llm_candidates() {
    let payload = command_mode_payload(
        " selected source ",
        " make it shorter ",
        [
            " first rewrite ".to_owned(),
            String::new(),
            "second rewrite".to_owned(),
        ],
    );

    assert_eq!(payload.commit_text, "first rewrite");
    assert_eq!(payload.candidates.len(), 4);
    assert_eq!(payload.candidates[0].text, "selected source");
    assert_eq!(payload.candidates[0].source.to_string(), "raw");
    assert_eq!(payload.candidates[1].text, "make it shorter");
    assert_eq!(payload.candidates[1].source.to_string(), "asr");
    assert_eq!(payload.candidates[2].text, "first rewrite");
    assert_eq!(payload.candidates[2].source.to_string(), "llm");
    assert_eq!(payload.candidates[3].text, "second rewrite");
    assert_eq!(payload.candidates[3].source.to_string(), "llm");
}

#[test]
fn command_mode_payload_falls_back_to_selected_text_without_llm() {
    let payload = command_mode_payload("selected source", "", Vec::<String>::new());

    assert_eq!(payload.commit_text, "selected source");
    assert_eq!(payload.candidates.len(), 1);
    assert_eq!(payload.candidates[0].text, "selected source");
    assert_eq!(payload.candidates[0].source.to_string(), "raw");
}

#[test]
fn command_text_request_serializes_scene_context() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        provider_id: Some("openai".to_owned()),
        model: Some("gpt".to_owned()),
        timeout_ms: Some(2_500),
        context_lines: 4,
        ..scene("rewrite", 2)
    };
    let request = CommandTextRequest::from_text_request(
        "cmd-adapter",
        &TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: Some("selection"),
        },
    );
    let value = serde_json::to_value(&request).unwrap();

    assert_eq!(value["adapter_id"], "cmd-adapter");
    assert_eq!(value["raw_text"], "raw text");
    assert_eq!(value["selected_text"], "selection");
    assert_eq!(value["scene"]["id"], "rewrite");
    assert_eq!(value["scene"]["prompt"], "polish");
    assert_eq!(value["scene"]["provider_id"], "openai");
    assert_eq!(value["scene"]["model"], "gpt");
    assert_eq!(value["scene"]["candidate_count"], 2);
    assert_eq!(value["scene"]["timeout_ms"], 2_500);
    assert_eq!(value["scene"]["context_lines"], 4);
}

#[test]
fn command_text_request_preserves_missing_selected_text() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("rewrite", 1)
    };
    let request = CommandTextRequest::from_text_request(
        "cmd-adapter",
        &TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        },
    );

    assert!(request.selected_text.is_none());
    let value = serde_json::to_value(&request).unwrap();
    assert!(value["selected_text"].is_null());
}

#[test]
fn openai_compatible_candidate_parser_extracts_first_choice_content_json() {
    let response = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({
                    "candidates": [" polished ", "", 7, "second"]
                }).to_string()
            }
        }]
    });

    assert_eq!(
        extract_openai_compatible_candidates(&response.to_string()),
        vec!["polished".to_owned(), "second".to_owned()]
    );
}

#[test]
fn openai_compatible_candidate_parser_returns_empty_for_invalid_shapes() {
    for response in [
            "not json".to_owned(),
            serde_json::json!({}).to_string(),
            serde_json::json!({"choices": []}).to_string(),
            serde_json::json!({"choices": [{"message": {"content": "not json"}}]}).to_string(),
            serde_json::json!({
                "choices": [{"message": {"content": serde_json::json!({"candidates": "no"}).to_string()}}]
            })
            .to_string(),
        ] {
            assert!(
                extract_openai_compatible_candidates(&response).is_empty(),
                "response should not yield candidates: {response}"
            );
        }
}

#[test]
fn openai_compatible_candidates_to_payload_uses_llm_source() {
    let payload =
        openai_compatible_candidates_to_payload(vec!["first".to_owned(), "second".to_owned()])
            .unwrap();

    assert_eq!(payload.commit_text, "first");
    assert_eq!(payload.candidates.len(), 2);
    assert_eq!(payload.candidates[0].text, "first");
    assert_eq!(payload.candidates[0].source.to_string(), "llm");
    assert_eq!(payload.candidates[1].text, "second");
    assert_eq!(payload.candidates[1].source.to_string(), "llm");
}

#[test]
fn openai_compatible_candidates_to_payload_returns_none_for_empty_candidates() {
    assert!(openai_compatible_candidates_to_payload(Vec::<String>::new()).is_none());
}

#[test]
fn openai_compatible_response_to_payload_parses_llm_candidates() {
    let response = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({
                    "candidates": [" first ", "second"]
                }).to_string()
            }
        }]
    });

    let payload = openai_compatible_response_to_payload(&response.to_string()).unwrap();

    assert_eq!(payload.commit_text, "first");
    assert_eq!(payload.candidates.len(), 2);
    assert_eq!(payload.candidates[0].source.to_string(), "llm");
    assert_eq!(payload.candidates[1].text, "second");
}

#[test]
fn openai_compatible_response_to_payload_returns_none_for_invalid_shapes() {
    assert!(openai_compatible_response_to_payload("not-json").is_none());
    assert!(
        openai_compatible_response_to_payload(
            &serde_json::json!({"choices": [{"message": {"content": "not-json"}}]}).to_string()
        )
        .is_none()
    );
}

#[test]
fn openai_compatible_extra_body_merge_ignores_protected_keys() {
    let mut request = serde_json::json!({
        "model": "model-a",
        "messages": [{"role": "user", "content": "prompt"}],
        "stream": false,
        "response_format": {"type": "json_object"},
        "temperature": 0.2
    });
    let extra_body = serde_json::json!({
        "messages": "bad override",
        "stream": true,
        "response_format": {"type": "text"},
        "temperature": 0.7,
        "top_p": 0.9,
        "enable_thinking": true
    });

    let ignored = merge_openai_compatible_extra_body(&mut request, &extra_body);

    assert_eq!(request["messages"][0]["content"], "prompt");
    assert_eq!(request["stream"], false);
    assert_eq!(request["response_format"]["type"], "json_object");
    assert_eq!(request["temperature"], 0.7);
    assert_eq!(request["top_p"], 0.9);
    assert_eq!(request["enable_thinking"], true);
    assert_eq!(ignored.len(), 3);
    assert!(ignored.iter().any(|key| key == "messages"));
    assert!(ignored.iter().any(|key| key == "stream"));
    assert!(ignored.iter().any(|key| key == "response_format"));
}

#[test]
fn openai_compatible_extra_body_merge_ignores_non_objects() {
    let mut request = serde_json::json!({"temperature": 0.2});

    assert!(merge_openai_compatible_extra_body(&mut request, &serde_json::json!([])).is_empty());
    assert_eq!(request["temperature"], 0.2);

    let mut not_object = serde_json::json!([]);
    assert!(
        merge_openai_compatible_extra_body(&mut not_object, &serde_json::json!({"top_p": 0.9}))
            .is_empty()
    );
    assert_eq!(not_object, serde_json::json!([]));
}

#[test]
fn command_text_response_maps_final_text_to_payload() {
    let payload = CommandTextResponse {
        payload: None,
        text: Some("polished".to_owned()),
        error: None,
    }
    .into_payload()
    .unwrap();

    assert_eq!(payload.commit_text, "polished");
    assert_eq!(payload.candidates[0].text, "polished");
}

#[test]
fn command_text_response_accepts_full_payload() {
    let response: CommandTextResponse = serde_json::from_str(
        r#"{"payload":{"commit_text":"choice","candidates":[{"text":"choice","source":"llm"}]}}"#,
    )
    .unwrap();
    let payload = response.into_payload().unwrap();

    assert_eq!(payload.commit_text, "choice");
    assert_eq!(payload.candidates[0].text, "choice");
    assert_eq!(payload.candidates[0].source.to_string(), "llm");
}

#[test]
fn command_text_response_normalizes_full_payload() {
    let response: CommandTextResponse =
        serde_json::from_str(r#"{"payload":{"commit_text":"choice","candidates":[]}}"#).unwrap();
    let payload = response.into_payload().unwrap();

    assert_eq!(payload.commit_text, "choice");
    assert_eq!(payload.candidates[0].text, "choice");
}

#[test]
fn command_text_response_prefers_error_over_payload() {
    let response: CommandTextResponse = serde_json::from_str(
        r#"{"payload":{"commit_text":"choice","candidates":[]},"error":"adapter boom"}"#,
    )
    .unwrap();
    let error = response.into_payload().unwrap_err();

    assert_eq!(error, TextError::AdapterFailed("adapter boom".to_owned()));
}

#[test]
fn command_text_response_prefers_payload_over_text() {
    let response: CommandTextResponse = serde_json::from_str(
        r#"{"payload":{"commit_text":"payload","candidates":[]},"text":"text fallback"}"#,
    )
    .unwrap();
    let payload = response.into_payload().unwrap();

    assert_eq!(payload.commit_text, "payload");
    assert_eq!(payload.candidates[0].text, "payload");
}

#[test]
fn command_text_response_accepts_failure_alias() {
    let response: CommandTextResponse =
        serde_json::from_str(r#"{"failure":"adapter boom"}"#).unwrap();
    let error = response.into_payload().unwrap_err();

    assert_eq!(error, TextError::AdapterFailed("adapter boom".to_owned()));
}

#[test]
fn command_text_response_rejects_blank_final_text() {
    let error = CommandTextResponse {
        payload: None,
        text: Some("   ".to_owned()),
        error: None,
    }
    .into_payload()
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message) if message.contains("missing final text")
    ));
}

#[test]
fn adapter_registry_indexes_command_adapters_from_config() {
    let registry = super::AdapterRegistry::from_configs(&[LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "vinput-postprocess".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([("MODE".to_owned(), "test".to_owned())]),
        working_dir: Some("/tmp/vinput".to_owned()),
        extra: std::collections::HashMap::default(),
    }]);

    assert_eq!(registry.len(), 1);
    assert!(registry.contains_command_adapter("cmd-adapter"));
    let adapter = registry
        .command_adapter("cmd-adapter")
        .expect("adapter should be indexed");
    assert_eq!(adapter.command(), "vinput-postprocess");
    assert_eq!(adapter.env().get("MODE").map(String::as_str), Some("test"));
    assert_eq!(adapter.working_dir(), Some("/tmp/vinput"));
    assert!(!registry.contains_command_adapter("missing"));
    assert!(registry.command_adapter("missing").is_none());
    assert_eq!(
        registry
            .single_command_adapter()
            .map(CommandTextAdapter::command),
        Some("vinput-postprocess")
    );
}

#[test]
fn adapter_registry_returns_no_single_adapter_for_empty_config() {
    let registry = super::AdapterRegistry::new();
    assert!(registry.single_command_adapter().is_none());
}

#[test]
fn adapter_registry_returns_no_single_adapter_for_multiple_configs() {
    let registry = super::AdapterRegistry::from_configs(&[
        LlmAdapterConfig {
            id: "first".to_owned(),
            command: "first-command".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        },
        LlmAdapterConfig {
            id: "second".to_owned(),
            command: "second-command".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        },
    ]);
    assert!(registry.single_command_adapter().is_none());
}

#[test]
fn command_text_processor_keeps_raw_scene_without_adapters() {
    let raw = scene(RAW_SCENE_ID, 0);
    let payload = CommandTextProcessor::from_configs(&[])
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &raw,
            selected_text: None,
        })
        .unwrap();

    assert_eq!(payload.commit_text, "raw text");
}

#[test]
fn command_text_processor_requires_adapter_for_prompted_scene() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let error = CommandTextProcessor::from_configs(&[])
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

    assert_eq!(error, TextError::AdapterRequired("polish".to_owned()));
}

#[test]
fn command_text_processor_rejects_ambiguous_adapters_despite_provider_id() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        provider_id: Some("first".to_owned()),
        ..scene("polish", 0)
    };
    let processor = CommandTextProcessor::from_configs_with_runner(
        &[
            LlmAdapterConfig {
                id: "first".to_owned(),
                command: "first-command".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                working_dir: None,
                extra: std::collections::HashMap::default(),
            },
            LlmAdapterConfig {
                id: "second".to_owned(),
                command: "second-command".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                working_dir: None,
                extra: std::collections::HashMap::default(),
            },
        ],
        EchoCommandRunner,
    );
    let error = processor
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

    assert_eq!(error, TextError::AmbiguousAdapter("polish".to_owned()));
}

#[test]
fn command_text_processor_delegates_to_single_adapter() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let processor = CommandTextProcessor::from_configs_with_runner(
        &[LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("MODE".to_owned(), "mock".to_owned())]),
            working_dir: Some("/tmp/vinput".to_owned()),
            extra: std::collections::HashMap::default(),
        }],
        EchoCommandRunner,
    );
    let payload = processor
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap();

    assert_eq!(
        payload.commit_text,
        "vinput-postprocess --json mock /tmp/vinput: raw text"
    );
}

#[test]
fn command_text_adapter_copies_typed_config() {
    let adapter = CommandTextAdapter::from_config(&LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "vinput-postprocess".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([("MODE".to_owned(), "test".to_owned())]),
        working_dir: Some("/tmp/vinput-text".to_owned()),
        extra: std::collections::HashMap::default(),
    });

    assert_eq!(adapter.id(), "cmd-adapter");
    assert_eq!(adapter.command(), "vinput-postprocess");
    assert_eq!(adapter.args(), ["--json"]);
    assert_eq!(adapter.env().get("MODE").map(String::as_str), Some("test"));
    assert_eq!(adapter.working_dir(), Some("/tmp/vinput-text"));
}

#[test]
fn command_text_adapter_delegates_to_injected_runner() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "vinput-postprocess".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([("MODE".to_owned(), "mock".to_owned())]),
        working_dir: Some("/tmp/vinput".to_owned()),
        extra: std::collections::HashMap::default(),
    };
    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        EchoCommandRunner,
    ))
    .finish(&TextRequest {
        raw_text: "hello",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(
        payload.commit_text,
        "vinput-postprocess --json mock /tmp/vinput: hello"
    );
}

#[test]
fn process_command_text_runner_writes_request_and_reads_response() {
    let mut capture_path = std::env::temp_dir();
    capture_path.push(format!(
        "vinput-command-text-request-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat > "$TEXT_REQUEST"; printf '%s\n' '{"text":"polished final"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "TEXT_REQUEST".to_owned(),
            capture_path.to_string_lossy().into_owned(),
        )]),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: Some("selection"),
    })
    .unwrap();
    assert_eq!(payload.commit_text, "polished final");

    let request: CommandTextRequest =
        serde_json::from_str(&std::fs::read_to_string(&capture_path).unwrap()).unwrap();
    std::fs::remove_file(&capture_path).unwrap();
    assert_eq!(request.adapter_id, "cmd-adapter");
    assert_eq!(request.raw_text, "raw text");
    assert_eq!(request.selected_text.as_deref(), Some("selection"));
    assert_eq!(request.scene.id, "polish");
    assert_eq!(request.scene.prompt.as_deref(), Some("polish"));
}

#[test]
fn process_command_text_runner_reports_nonzero_exit() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; echo adapter boom >&2; exit 7".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("exited with") && message.contains("adapter boom")
    ));
}

#[test]
fn process_command_text_runner_reports_missing_program() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: format!("vinput-missing-text-adapter-{}", std::process::id()),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("failed to spawn text adapter `cmd-adapter`")
    ));
}

#[test]
fn process_command_text_runner_rejects_bad_json() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "cat >/dev/null; printf not-json".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("failed to decode text adapter response")
    ));
}

#[test]
fn process_command_text_runner_maps_helper_error_response() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s\n' '{"error":"adapter failed"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert_eq!(error, TextError::AdapterFailed("adapter failed".to_owned()));
}

#[test]
fn process_command_text_runner_reads_payload_response() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s\n' '{"payload":{"commit_text":"payload final","candidates":[{"text":"payload final","source":"llm"}]}}'"#.to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "payload final");
    assert_eq!(payload.candidates[0].text, "payload final");
    assert_eq!(payload.candidates[0].source.to_string(), "llm");
}

#[test]
fn process_command_text_runner_reports_early_exit() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            "echo early adapter boom >&2; exit 9".to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("exited with")
                && message.contains("early adapter boom")
                && !message.contains("failed to write")
    ));
}

#[test]
fn process_command_text_runner_reports_empty_stderr_exit_cleanly() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat >/dev/null; exit 7".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    };

    let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert!(matches!(
        error,
        TextError::AdapterFailed(message)
            if message.contains("exited with") && !message.ends_with(':')
    ));
}

#[test]
fn process_command_text_runner_uses_working_dir() {
    let mut work_dir = std::env::temp_dir();
    work_dir.push(format!(
        "vinput-command-text-workdir-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&work_dir).unwrap();
    let mut capture_path = std::env::temp_dir();
    capture_path.push(format!(
        "vinput-command-text-cwd-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let config = LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"pwd > "$TEXT_CWD"; cat >/dev/null; printf '%s\n' '{"text":"cwd final"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "TEXT_CWD".to_owned(),
            capture_path.to_string_lossy().into_owned(),
        )]),
        working_dir: Some(work_dir.to_string_lossy().into_owned()),
        extra: std::collections::HashMap::default(),
    };

    let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
        &config,
        ProcessCommandTextRunner,
    ))
    .finish(&TextRequest {
        raw_text: "raw text",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap();

    assert_eq!(payload.commit_text, "cwd final");
    assert_eq!(
        std::fs::read_to_string(&capture_path).unwrap().trim(),
        work_dir.to_string_lossy()
    );
    std::fs::remove_file(&capture_path).unwrap();
    std::fs::remove_dir(&work_dir).unwrap();
}

#[test]
fn command_text_adapter_returns_unsupported_until_runner_lands() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let error = LlmTextProcessor::new(CommandTextAdapter::new(
        "vinput-postprocess",
        vec!["--json".to_owned()],
    ))
    .finish(&TextRequest {
        raw_text: "hello",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();

    assert_eq!(error, TextError::UnsupportedAdapter("polish".to_owned()));
}

#[test]
fn llm_text_processor_keeps_noop_scene_raw() {
    let noop = scene("noop", 0);
    let payload = LlmTextProcessor::new(UnsupportedTextAdapter::new())
        .finish(&TextRequest {
            raw_text: "hello",
            scene: &noop,
            selected_text: None,
        })
        .unwrap();
    assert_eq!(payload.commit_text, "hello");
}

#[test]
fn llm_text_processor_delegates_prompted_scene_to_adapter() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let error = LlmTextProcessor::new(UnsupportedTextAdapter::new())
        .finish(&TextRequest {
            raw_text: "hello",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();
    assert_eq!(error, TextError::UnsupportedAdapter("polish".to_owned()));
}

#[test]
fn llm_text_processor_delegates_command_scene_to_adapter() {
    let command = scene(COMMAND_SCENE_ID, 0);
    let error = LlmTextProcessor::new(UnsupportedTextAdapter::new())
        .finish(&TextRequest {
            raw_text: "replace it",
            scene: &command,
            selected_text: Some("selected source"),
        })
        .unwrap_err();
    assert_eq!(
        error,
        TextError::UnsupportedAdapter(COMMAND_SCENE_ID.to_owned())
    );
}

#[test]
fn command_scene_requires_adapter_in_production_finisher() {
    let command = scene(COMMAND_SCENE_ID, 0);
    let error = TextFinisher::finish(&TextRequest {
        raw_text: "replace it",
        scene: &command,
        selected_text: Some("selected source"),
    })
    .unwrap_err();
    assert_eq!(
        error,
        TextError::AdapterRequired(COMMAND_SCENE_ID.to_owned())
    );
}

#[test]
fn mock_processor_handles_command_scene_with_selected_text() {
    let command = scene(COMMAND_SCENE_ID, 1);
    let payload = MockTextProcessor::new()
        .finish(&TextRequest {
            raw_text: "replace it",
            scene: &command,
            selected_text: Some("selected source"),
        })
        .unwrap();
    assert_eq!(
        payload.commit_text,
        "mock command result for: selected source"
    );
}

#[test]
fn mock_processor_handles_command_scene_without_selected_text() {
    let command = scene(COMMAND_SCENE_ID, 1);
    let payload = MockTextProcessor::new()
        .finish(&TextRequest {
            raw_text: "replace it",
            scene: &command,
            selected_text: None,
        })
        .unwrap();
    assert_eq!(payload.commit_text, "mock command result: replace it");
}

#[test]
fn candidate_scene_requires_future_adapter() {
    let fancy = scene("rewrite", 2);
    let error = TextFinisher::finish(&TextRequest {
        raw_text: "hello",
        scene: &fancy,
        selected_text: None,
    })
    .unwrap_err();
    assert_eq!(error, TextError::AdapterRequired("rewrite".to_owned()));
}

#[test]
fn prompted_scene_requires_future_adapter() {
    let prompted = SceneDefinition {
        prompt: Some("polish".to_owned()),
        ..scene("polish", 0)
    };
    let error = TextFinisher::finish(&TextRequest {
        raw_text: "hello",
        scene: &prompted,
        selected_text: None,
    })
    .unwrap_err();
    assert_eq!(error, TextError::AdapterRequired("polish".to_owned()));
}

#[test]
fn provider_bound_scene_requires_future_adapter() {
    let provider_bound = SceneDefinition {
        provider_id: Some("openai".to_owned()),
        model: Some("gpt-test".to_owned()),
        ..scene("provider-scene", 0)
    };
    let error = TextFinisher::finish(&TextRequest {
        raw_text: "hello",
        scene: &provider_bound,
        selected_text: None,
    })
    .unwrap_err();
    assert_eq!(
        error,
        TextError::AdapterRequired("provider-scene".to_owned())
    );
}

#[test]
fn timeout_scene_requires_future_adapter() {
    let timeout_scene = SceneDefinition {
        timeout_ms: Some(2500),
        ..scene("timeout-scene", 0)
    };
    let error = TextFinisher::finish(&TextRequest {
        raw_text: "hello",
        scene: &timeout_scene,
        selected_text: None,
    })
    .unwrap_err();
    assert_eq!(
        error,
        TextError::AdapterRequired("timeout-scene".to_owned())
    );
}

#[test]
fn context_scene_requires_future_adapter() {
    let context_scene = SceneDefinition {
        context_lines: 2,
        ..scene("context-scene", 0)
    };
    let error = TextFinisher::finish(&TextRequest {
        raw_text: "hello",
        scene: &context_scene,
        selected_text: None,
    })
    .unwrap_err();
    assert_eq!(
        error,
        TextError::AdapterRequired("context-scene".to_owned())
    );
}

#[test]
fn mock_processor_handles_timeout_scene() {
    let timeout_scene = SceneDefinition {
        timeout_ms: Some(2500),
        ..scene("timeout-scene", 0)
    };
    let payload = MockTextProcessor::new()
        .finish(&TextRequest {
            raw_text: "hello",
            scene: &timeout_scene,
            selected_text: None,
        })
        .unwrap();
    assert_eq!(payload.commit_text, "mock postprocess result: hello");
}

#[test]
fn mock_processor_handles_candidate_scene() {
    let fancy = scene("rewrite", 2);
    let payload = MockTextProcessor::new()
        .finish(&TextRequest {
            raw_text: "hello",
            scene: &fancy,
            selected_text: None,
        })
        .unwrap();
    assert_eq!(payload.commit_text, "mock postprocess result: hello");
}
