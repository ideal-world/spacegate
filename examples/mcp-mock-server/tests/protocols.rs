use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{header, Method, Request, StatusCode};
use mcp_mock_server::{handle_request, MockState};
use serde_json::{json, Value};

/// A Streamable HTTP initialize response must create and expose a session ID.
#[tokio::test]
async fn streamable_initialize_returns_a_session_id() {
    let response = handle_request(
        Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .body(Full::new(Bytes::from_static(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)))
            .expect("request"),
        MockState::new(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("Mcp-Session-Id").and_then(|value| value.to_str().ok()), Some("streamable-1"));
    assert_eq!(response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()), Some("application/json"));
}

/// A Streamable HTTP tool call can exercise a two-frame SSE upstream response.
#[tokio::test]
async fn streamable_tool_call_returns_two_sse_frames() {
    let mut response = handle_request(
        Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .body(Full::new(Bytes::from_static(br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"stream":true}}"#)))
            .expect("request"),
        MockState::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let first = response.body_mut().frame().await.expect("first SSE frame").expect("first SSE data").into_data().expect("first SSE bytes");
    assert_eq!(
        sse_json_data(&first),
        json!({"jsonrpc": "2.0", "id": 2, "result": {"content": [{"type": "text", "text": "mock first chunk"}]}})
    );
    let second = response.body_mut().frame().await.expect("second SSE frame").expect("second SSE data").into_data().expect("second SSE bytes");
    assert_eq!(second, Bytes::from_static(b"data: [DONE]\n\n"));
    assert!(response.body_mut().frame().await.is_none());
}

/// A Streamable HTTP GET can exercise an upstream-originated SSE event stream.
#[tokio::test]
async fn streamable_get_returns_an_sse_event() {
    let mut response = handle_request(
        Request::builder().method(Method::GET).uri("/mcp").body(Full::new(Bytes::new())).expect("request"),
        MockState::new(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let event = response.body_mut().frame().await.expect("SSE frame").expect("SSE data").into_data().expect("SSE bytes");
    assert_eq!(
        sse_json_data(&event),
        json!({"jsonrpc": "2.0", "method": "notifications/message", "params": {"level": "info", "data": "mock stream opened"}})
    );
}

/// A Streamable HTTP DELETE must invalidate the session returned by initialize.
#[tokio::test]
async fn streamable_delete_removes_an_initialized_session() {
    let state = MockState::new();
    let initialized = handle_request(
        Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .body(Full::new(Bytes::from_static(br#"{"jsonrpc":"2.0","id":5,"method":"initialize","params":{}}"#)))
            .expect("request"),
        state.clone(),
    )
    .await;
    let session_id = initialized.headers().get("Mcp-Session-Id").expect("session header").clone();

    let deleted = handle_request(
        Request::builder().method(Method::DELETE).uri("/mcp").header("Mcp-Session-Id", &session_id).body(Full::new(Bytes::new())).expect("request"),
        state.clone(),
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let deleted_again = handle_request(
        Request::builder().method(Method::DELETE).uri("/mcp").header("Mcp-Session-Id", session_id).body(Full::new(Bytes::new())).expect("request"),
        state,
    )
    .await;
    assert_eq!(deleted_again.status(), StatusCode::NOT_FOUND);
}

/// A Streamable HTTP DELETE without a session ID is an invalid session operation.
#[tokio::test]
async fn streamable_delete_requires_a_session_header() {
    let response = handle_request(
        Request::builder().method(Method::DELETE).uri("/mcp").body(Full::new(Bytes::new())).expect("request"),
        MockState::new(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A Legacy message POST must return 202 and write its result into the already-open SSE body.
#[tokio::test]
async fn legacy_message_post_writes_to_the_open_sse_body() {
    let state = MockState::new();
    let mut sse = handle_request(
        Request::builder().method(Method::GET).uri("/sse").body(Full::new(Bytes::new())).expect("request"),
        state.clone(),
    )
    .await;
    let endpoint = sse.body_mut().frame().await.expect("endpoint frame").expect("endpoint data").into_data().expect("endpoint bytes");
    assert_eq!(endpoint, Bytes::from_static(b"event: endpoint\ndata: /message?sessionId=legacy-1\n\n"));

    let message = handle_request(
        Request::builder()
            .method(Method::POST)
            .uri("/message?sessionId=legacy-1")
            .body(Full::new(Bytes::from_static(br#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#)))
            .expect("request"),
        state,
    )
    .await;
    assert_eq!(message.status(), StatusCode::ACCEPTED);

    let response = sse.body_mut().frame().await.expect("Legacy message frame").expect("Legacy message data").into_data().expect("message bytes");
    assert_eq!(
        sse_json_data(&response),
        json!({"jsonrpc": "2.0", "id": 3, "result": {"tools": [{"name": "mock.echo", "description": "Returns the supplied input", "inputSchema": {"type": "object"}}]}})
    );
}

/// An unknown Legacy SSE session must not be treated as a successful message delivery.
#[tokio::test]
async fn legacy_message_post_rejects_an_unknown_session() {
    let response = handle_request(
        Request::builder()
            .method(Method::POST)
            .uri("/message?sessionId=missing")
            .body(Full::new(Bytes::from_static(br#"{"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}}"#)))
            .expect("request"),
        MockState::new(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Parses the JSON payload carried by one complete SSE data event.
fn sse_json_data(event: &Bytes) -> Value {
    let event = std::str::from_utf8(event).expect("UTF-8 SSE event");
    let payload = event.lines().find_map(|line| line.strip_prefix("data: ")).expect("JSON SSE data event");
    serde_json::from_str(payload).expect("JSON-RPC SSE payload")
}
