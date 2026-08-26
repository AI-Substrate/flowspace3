//! A local HTTP service the provider tests answer from.
//!
//! This is a **fake**, not a mock (workshop 001 rule 5): a real axum server on
//! `127.0.0.1:0` that records what it was asked and replies with whatever the
//! test decided. It needs no credential, no network and no account, so every
//! request-shape and error-mapping assertion in this crate runs keyless in CI.
//!
//! It lives in `tests/common/` because two test binaries need it. When it is
//! promoted into `fs3-testkit` (where the shipped fakes live) every future
//! adapter gets it for free — that promotion is the point of keeping it
//! provider-agnostic here.

// Each test binary compiles this module separately, so a helper only one of
// them needs looks dead to the other. That is a property of `tests/common`,
// not a defect in the helper.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};

/// One request, as the service saw it.
#[derive(Clone, Debug)]
pub struct Captured {
    pub path: String,
    pub query: String,
    pub headers: HeaderMap,
    pub body: serde_json::Value,
}

impl Captured {
    /// A header value, or `None` when the request did not carry it.
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .map(|value| value.to_str().expect("an ascii header").to_string())
    }

    /// The `response_format.type` this request asked for, if any.
    pub fn response_format(&self) -> Option<&str> {
        self.body.get("response_format")?.get("type")?.as_str()
    }
}

/// What the service should answer. Taken per request so a test can make the
/// reply depend on what was asked — which is how a downgrade is provable.
type Answer = Arc<dyn Fn(&Captured) -> (StatusCode, String) + Send + Sync>;

#[derive(Clone)]
struct StubState {
    answer: Answer,
    seen: Arc<Mutex<Vec<Captured>>>,
}

/// A local endpoint that records every request and answers on demand.
pub struct StubServer {
    pub endpoint: String,
    seen: Arc<Mutex<Vec<Captured>>>,
}

impl StubServer {
    /// A server whose reply is computed from the request.
    pub async fn answering_with(
        answer: impl Fn(&Captured) -> (StatusCode, String) + Send + Sync + 'static,
    ) -> Self {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let state = StubState {
            answer: Arc::new(answer),
            seen: Arc::clone(&seen),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an ephemeral port");
        let endpoint = format!("http://{}", listener.local_addr().expect("a bound address"));

        let app = Router::new().fallback(record).with_state(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self { endpoint, seen }
    }

    /// A server that answers every route the same way.
    pub async fn answering(status: StatusCode, body: &str) -> Self {
        let body = body.to_string();
        Self::answering_with(move |_| (status, body.clone())).await
    }

    /// A server that answers every route with `200` and `body`.
    pub async fn ok(body: &str) -> Self {
        Self::answering(StatusCode::OK, body).await
    }

    /// A server that refuses schema-constrained requests the way an endpoint
    /// too old to understand them does, and answers anything else with `body`.
    ///
    /// The refusal text is Azure's, and OpenAI-compatible servers phrase it
    /// their own way — what they share is a client error naming the parameter,
    /// which is exactly what the adapter keys its downgrade on.
    pub async fn rejecting_structured_output(body: &str) -> Self {
        let body = body.to_string();
        Self::answering_with(move |request| {
            if request.response_format() == Some("json_schema") {
                (
                    StatusCode::BAD_REQUEST,
                    r#"{"error":{"code":"unknown_parameter","message":"Unrecognized request argument supplied: response_format.json_schema"}}"#
                        .to_string(),
                )
            } else {
                (StatusCode::OK, body.clone())
            }
        })
        .await
    }

    /// Every request the service has seen, in order.
    pub fn requests(&self) -> Vec<Captured> {
        self.seen.lock().expect("no panicking handler").clone()
    }

    /// The single request the service saw. Panics if there was not exactly one.
    pub fn only_request(&self) -> Captured {
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "expected exactly one request");
        requests.into_iter().next().expect("length checked")
    }
}

async fn record(State(state): State<StubState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.expect("a readable body");
    let captured = Captured {
        path: parts.uri.path().to_string(),
        query: parts.uri.query().unwrap_or_default().to_string(),
        headers: parts.headers,
        body: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    };

    let (status, reply) = (state.answer)(&captured);
    state.seen.lock().expect("no poisoned lock").push(captured);

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(reply))
        .expect("a valid response")
}
