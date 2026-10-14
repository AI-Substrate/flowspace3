//! What the Decisions client puts on the wire, and what it makes of the
//! service's answers and refusals — proved without OpenAI.
//!
//! The stub is a fake service — see `tests/common/mod.rs`. No key, no network.

use axum::http::StatusCode;
use fs3_providers::{
    Answer, ChoiceOption, ChoiceValue, DECISIONS_DEFAULT_MODEL, DecisionInput, DecisionRequest,
    DecisionsClient, ImageDetail, InputMessage, InputPart, Question,
};

mod common;
use common::StubServer;

const DECISION: &str = r#"{"model":"gpt-6-luna","answers":[
    {"type":"predicate","name":"complaint","probability":0.97},
    {"type":"choice","name":"dept","choice":"billing","confidence":0.93,
     "probabilities":[{"value":"billing","probability":0.95},{"value":"other","probability":0.05}]}],
  "usage":{"input_tokens":120,"output_tokens":0,"total_tokens":120}}"#;

fn client(endpoint: &str) -> DecisionsClient {
    DecisionsClient::new(Some(endpoint.to_string()), "test-key")
}

fn request() -> DecisionRequest {
    DecisionRequest {
        model: DECISIONS_DEFAULT_MODEL.into(),
        input: DecisionInput::Messages(vec![InputMessage {
            content: vec![
                InputPart::text("I was charged twice for my order."),
                InputPart::image_bytes("image/png", b"not-really-a-png", Some(ImageDetail::Low)),
            ],
        }]),
        questions: vec![
            Question::Predicate {
                name: Some("complaint".into()),
                instructions: "Is this a complaint?".into(),
            },
            Question::Choice {
                name: Some("dept".into()),
                instructions: "Which department?".into(),
                choices: vec![
                    ChoiceOption {
                        value: ChoiceValue::Text("billing".into()),
                        description: None,
                    },
                    ChoiceOption {
                        value: ChoiceValue::Text("other".into()),
                        description: None,
                    },
                ],
            },
        ],
        safety_identifier: Some("user-1234".into()),
    }
}

/// Bearer auth, the decisions route, and the body exactly as the caller built
/// it — the client adds nothing and drops nothing.
#[tokio::test]
async fn the_request_carries_the_key_the_route_and_the_body() {
    let stub = StubServer::ok(DECISION).await;

    client(&stub.endpoint)
        .create(&request())
        .await
        .expect("the stub answers");

    let captured = stub.only_request();
    assert_eq!(captured.path, "/decisions");
    assert_eq!(
        captured.header("authorization").as_deref(),
        Some("Bearer test-key")
    );
    assert_eq!(
        captured.body,
        serde_json::to_value(request()).expect("the request serialises")
    );
    assert_eq!(captured.body["input"][0]["role"], "user");
    assert_eq!(captured.body["input"][0]["content"][1]["detail"], "low");
    assert!(
        captured.body["input"][0]["content"][1]["image_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("data:image/png;base64,")),
        "images are inlined as data URLs"
    );
}

#[tokio::test]
async fn answers_come_back_typed_and_in_question_order() {
    let stub = StubServer::ok(DECISION).await;

    let decision = client(&stub.endpoint)
        .create(&request())
        .await
        .expect("the stub answers");

    assert_eq!(decision.model, "gpt-6-luna");
    assert!(matches!(
        decision.answers[0],
        Answer::Predicate { probability, .. } if (probability - 0.97).abs() < f64::EPSILON
    ));
    assert!(matches!(
        &decision.answers[1],
        Answer::Choice { choice: ChoiceValue::Text(choice), .. } if choice == "billing"
    ));
    assert_eq!(decision.usage.expect("usage was sent").output_tokens, 0);
}

/// A rejected request is the caller's to fix: one attempt, the service's own
/// sentence, the field it blamed, and not retryable.
#[tokio::test]
async fn a_rejected_request_names_the_reason_and_is_not_retried() {
    let stub = StubServer::answering(
        StatusCode::BAD_REQUEST,
        r#"{"error":{"message":"Question names must be unique within the request.","type":"invalid_request_error","param":"questions[1].name","code":null}}"#,
    )
    .await;

    let error = client(&stub.endpoint)
        .create(&request())
        .await
        .expect_err("a 400 is an error");

    assert_eq!(stub.requests().len(), 1, "a 400 must not be retried");
    assert_eq!(error.status, Some(400));
    assert_eq!(
        error.message,
        "Question names must be unique within the request."
    );
    assert_eq!(error.param.as_deref(), Some("questions[1].name"));
    assert!(!error.retryable);
    assert!(
        error.to_string().contains("(param `questions[1].name`)"),
        "{error}"
    );
}

/// A busy service is retried until it answers; the caller never sees the 429.
#[tokio::test]
async fn a_transient_429_is_absorbed() {
    let stub =
        StubServer::failing_then(1, StatusCode::TOO_MANY_REQUESTS, Some("0"), DECISION).await;

    client(&stub.endpoint)
        .create(&request())
        .await
        .expect("the retry succeeds");

    assert_eq!(stub.requests().len(), 2);
}

/// A service that stays busy is reported as busy — retryable, with its wait.
#[tokio::test]
async fn a_persistent_429_is_reported_as_retryable() {
    let stub =
        StubServer::failing_then(9, StatusCode::TOO_MANY_REQUESTS, Some("0"), DECISION).await;

    let error = client(&stub.endpoint)
        .create(&request())
        .await
        .expect_err("still busy");

    assert_eq!(error.status, Some(429));
    assert!(error.retryable);
}

/// A body that is not a decision is a failure, never an empty answer list.
#[tokio::test]
async fn an_unreadable_body_is_an_error() {
    let stub = StubServer::ok(r#"{"choices":[]}"#).await;

    let error = client(&stub.endpoint)
        .create(&request())
        .await
        .expect_err("not a decision");

    assert_eq!(error.status, None);
    assert!(error.message.contains("unreadable response"), "{error}");
}
