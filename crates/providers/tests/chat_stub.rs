//! Tool-calling chat wire behaviour, proved against a real local HTTP service.
//!
//! The request assertions pin the two Azure deployment failures that are easy
//! to miss in a generic OpenAI client: mutually exclusive authentication and
//! GPT-5's completion-cap parameter name.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use azure_core::{
    credentials::{AccessToken, TokenCredential, TokenRequestOptions},
    time::OffsetDateTime,
};
use fs3_core::ChatProvider;
use fs3_providers::{
    AzureCredential, AzureOpenAiChatClient, AzureOpenAiConfig, COGNITIVE_SERVICES_SCOPE,
    ChatCompletionRequest, ChatFunctionCall, ChatMessage, ChatRole, ChatTool, ChatToolCall,
    ChatToolDefinition,
};
use serde_json::json;

mod common;
use common::StubServer;

#[derive(Debug)]
struct ScriptedCredential {
    token: String,
    calls: AtomicUsize,
    scopes: Mutex<Vec<String>>,
}

impl ScriptedCredential {
    fn new(token: &str) -> Arc<Self> {
        Arc::new(Self {
            token: token.to_string(),
            calls: AtomicUsize::new(0),
            scopes: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl TokenCredential for ScriptedCredential {
    async fn get_token(
        &self,
        scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.scopes
            .lock()
            .expect("no poisoned lock")
            .extend(scopes.iter().map(|scope| scope.to_string()));
        Ok(AccessToken::new(
            self.token.clone(),
            OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600),
        ))
    }
}

fn config(endpoint: &str, credential: AzureCredential) -> AzureOpenAiConfig {
    AzureOpenAiConfig::new(endpoint, "agent", "2024-12-01-preview", credential)
}

fn search_tool() -> ChatTool {
    ChatTool::function(ChatToolDefinition::new(
        "search_code",
        "Search indexed source code",
        json!({
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"]
        }),
    ))
}

fn request(messages: Vec<ChatMessage>) -> ChatCompletionRequest {
    ChatCompletionRequest::new(messages, vec![search_tool()], 2048)
}

const PLAIN_RESPONSE: &str =
    r#"{"choices":[{"message":{"role":"assistant","content":"The answer."}}]}"#;
const TOOL_RESPONSE: &str = r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"search_code","arguments":"{\"query\":\"retry policy\"}"}}]}}]}"#;
const USAGE_RESPONSE: &str = r#"{"choices":[{"message":{"role":"assistant","content":"Measured."}}],"usage":{"prompt_tokens":13,"completion_tokens":8,"total_tokens":21}}"#;
const EXTRA_USAGE_RESPONSE: &str = r#"{"choices":[{"message":{"role":"assistant","content":"Measured."}}],"usage":{"prompt_tokens":13,"completion_tokens":8,"total_tokens":21,"prompt_tokens_details":{"cached_tokens":5},"future_counter":34}}"#;

#[tokio::test]
async fn an_api_key_chat_request_carries_tools_replayed_messages_and_the_gpt_five_cap() {
    let stub = StubServer::ok(PLAIN_RESPONSE).await;
    let client =
        AzureOpenAiChatClient::new(config(&stub.endpoint, AzureCredential::api_key("test-key")));
    let call = ChatToolCall {
        id: "call-1".to_string(),
        kind: "function".to_string(),
        function: ChatFunctionCall {
            name: "search_code".to_string(),
            arguments: r#"{"query":"retry policy"}"#.to_string(),
        },
    };

    client
        .complete(&request(vec![
            ChatMessage::user("Find retry handling"),
            ChatMessage::assistant(None, vec![call]),
            ChatMessage::tool("call-1", r#"{"hits":2}"#),
        ]))
        .await
        .expect("the stub answers");

    let sent = stub.only_request();
    assert_eq!(sent.path, "/openai/deployments/agent/chat/completions");
    assert_eq!(sent.query, "api-version=2024-12-01-preview");
    assert_eq!(sent.header("api-key").as_deref(), Some("test-key"));
    assert_eq!(sent.header("authorization"), None);
    assert_eq!(sent.body["tools"][0]["type"], "function");
    assert_eq!(sent.body["tools"][0]["function"]["name"], "search_code");
    assert_eq!(
        sent.body["tools"][0]["function"]["parameters"]["required"][0],
        "query"
    );
    assert_eq!(sent.body["messages"][1]["tool_calls"][0]["id"], "call-1");
    assert_eq!(sent.body["messages"][2]["role"], "tool");
    assert_eq!(sent.body["messages"][2]["tool_call_id"], "call-1");
    assert_eq!(sent.body["max_completion_tokens"], 2048);
    assert!(
        sent.body.get("max_tokens").is_none(),
        "GPT-5 rejects the legacy max_tokens parameter: {}",
        sent.body
    );
    assert!(
        sent.body.get("temperature").is_none(),
        "GPT-5 rejects non-default temperature: {}",
        sent.body
    );
}

#[tokio::test]
async fn a_tool_call_only_reply_keeps_null_content_and_json_text_arguments() {
    let stub = StubServer::ok(TOOL_RESPONSE).await;
    let client = AzureOpenAiChatClient::new(config(&stub.endpoint, AzureCredential::api_key("k")));

    let response = client
        .complete(&request(vec![ChatMessage::user("Find it")]))
        .await
        .expect("the stub answers");
    let message = &response.choices[0].message;

    assert_eq!(message.role, ChatRole::Assistant);
    assert_eq!(message.content, None);
    assert_eq!(message.tool_calls.len(), 1);
    assert_eq!(message.tool_calls[0].id, "call-1");
    assert_eq!(message.tool_calls[0].function.name, "search_code");
    assert_eq!(
        message.tool_calls[0].function.arguments,
        r#"{"query":"retry policy"}"#
    );
}

#[tokio::test]
async fn a_plain_content_reply_deserializes_without_tool_calls() {
    let stub = StubServer::ok(PLAIN_RESPONSE).await;
    let client = AzureOpenAiChatClient::new(config(&stub.endpoint, AzureCredential::api_key("k")));

    let response = client
        .complete(&request(vec![ChatMessage::user("Answer directly")]))
        .await
        .expect("the stub answers");
    let message = &response.choices[0].message;

    assert_eq!(message.content.as_deref(), Some("The answer."));
    assert!(message.tool_calls.is_empty());
    assert_eq!(message.tool_call_id, None);
}

#[tokio::test]
async fn reported_total_usage_reaches_the_provider_neutral_turn() {
    let stub = StubServer::ok(USAGE_RESPONSE).await;
    let client = AzureOpenAiChatClient::new(config(&stub.endpoint, AzureCredential::api_key("k")));

    let turn = client
        .turn(&[fs3_core::ChatMessage::User("Answer".to_string())], &[])
        .await
        .expect("the stub answers");

    assert_eq!(turn.tokens_used, Some(21));
}

#[tokio::test]
async fn a_reply_without_usage_keeps_token_cost_unknown() {
    let stub = StubServer::ok(PLAIN_RESPONSE).await;
    let client = AzureOpenAiChatClient::new(config(&stub.endpoint, AzureCredential::api_key("k")));

    let turn = client
        .turn(&[fs3_core::ChatMessage::User("Answer".to_string())], &[])
        .await
        .expect("the stub answers");

    assert_eq!(turn.tokens_used, None);
}

#[tokio::test]
async fn extra_usage_fields_do_not_break_a_chat_turn() {
    let stub = StubServer::ok(EXTRA_USAGE_RESPONSE).await;
    let client = AzureOpenAiChatClient::new(config(&stub.endpoint, AzureCredential::api_key("k")));

    let turn = client
        .turn(&[fs3_core::ChatMessage::User("Answer".to_string())], &[])
        .await
        .expect("the stub answers");

    assert_eq!(turn.tokens_used, Some(21));
}

#[tokio::test]
async fn an_entra_chat_request_sends_only_the_cognitive_services_bearer_token() {
    let stub = StubServer::ok(PLAIN_RESPONSE).await;
    let credential = ScriptedCredential::new("entra-token");
    let client = AzureOpenAiChatClient::new(config(
        &stub.endpoint,
        AzureCredential::entra(Arc::clone(&credential) as Arc<dyn TokenCredential>),
    ));

    client
        .complete(&request(vec![ChatMessage::user("Answer")]))
        .await
        .expect("the stub answers");

    let sent = stub.only_request();
    assert_eq!(
        sent.header("authorization").as_deref(),
        Some("Bearer entra-token")
    );
    assert_eq!(sent.header("api-key"), None);
    assert_eq!(credential.calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        credential
            .scopes
            .lock()
            .expect("no poisoned lock")
            .as_slice(),
        [COGNITIVE_SERVICES_SCOPE.to_string()]
    );
}
