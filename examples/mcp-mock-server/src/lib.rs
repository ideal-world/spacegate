use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use bytes::Bytes;
use futures_util::stream;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Body, Frame},
    header, Method, Request, Response, StatusCode,
};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

/// Response body type used by finite JSON and ongoing SSE mock responses.
pub type MockBody = UnsyncBoxBody<Bytes, Infallible>;

/// In-memory state shared by all mock MCP connections.
#[derive(Clone, Default)]
pub struct MockState {
    /// Active Legacy SSE session senders, keyed by the endpoint session ID.
    legacy_sessions: Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>,
    /// Active Streamable HTTP session IDs accepted by DELETE /mcp.
    streamable_sessions: Arc<Mutex<HashSet<String>>>,
    /// Monotonic source for deterministic mock session IDs.
    next_session: Arc<AtomicUsize>,
}

/// A client-facing Legacy SSE connection and its server-generated session ID.
pub struct LegacySession {
    /// Session ID embedded in the endpoint event sent to the Legacy SSE client.
    pub id: String,
    /// Events that must be written to this client's existing SSE connection.
    events: mpsc::Receiver<String>,
}

impl MockState {
    /// Creates an empty registry for mock MCP connections.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a Legacy SSE connection and queues its endpoint discovery event.
    pub async fn open_legacy_session(&self) -> LegacySession {
        let session_id = format!("legacy-{}", self.next_session.fetch_add(1, Ordering::Relaxed) + 1);
        let (sender, receiver) = mpsc::channel(8);
        let endpoint = format!("event: endpoint\ndata: /message?sessionId={session_id}\n\n");
        sender.send(endpoint).await.expect("new Legacy SSE receiver is available");
        self.legacy_sessions.lock().await.insert(session_id.clone(), sender);
        LegacySession { id: session_id, events: receiver }
    }

    /// Returns the next event queued for this client's existing SSE connection.
    pub async fn next_event(session: &mut LegacySession) -> Option<String> {
        session.events.recv().await
    }

    /// Delivers an encoded JSON-RPC response to a Legacy SSE session.
    pub async fn send_legacy_response(&self, session_id: &str, response: String) -> bool {
        let sender = self.legacy_sessions.lock().await.get(session_id).cloned();
        let Some(sender) = sender else {
            return false;
        };
        let event = format!("event: message\ndata: {response}\n\n");
        if sender.send(event).await.is_ok() {
            return true;
        }
        self.legacy_sessions.lock().await.remove(session_id);
        false
    }

    /// Creates a Streamable HTTP session ID returned by initialize.
    pub async fn open_streamable_session(&self) -> String {
        let session_id = format!("streamable-{}", self.next_session.fetch_add(1, Ordering::Relaxed) + 1);
        self.streamable_sessions.lock().await.insert(session_id.clone());
        session_id
    }

    /// Removes one active Streamable HTTP session after a successful DELETE request.
    pub async fn remove_streamable_session(&self, session_id: &str) -> bool {
        self.streamable_sessions.lock().await.remove(session_id)
    }
}

impl LegacySession {
    /// Returns the next event queued for this client's existing SSE connection.
    pub async fn next_event(&mut self) -> Option<String> {
        MockState::next_event(self).await
    }

    /// Transfers the pending events to the HTTP response body that owns this SSE connection.
    pub fn into_events(self) -> mpsc::Receiver<String> {
        self.events
    }
}

/// Handles one HTTP request sent to the local mock MCP upstream.
pub async fn handle_request<B>(request: Request<B>, state: MockState) -> Response<MockBody>
where
    B: Body<Data = Bytes> + Send + 'static,
{
    let (parts, body) = request.into_parts();
    let method = parts.method;
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().map(str::to_string);
    let session_id = parts.headers.get("Mcp-Session-Id").and_then(|value| value.to_str().ok()).map(str::to_string);
    let body = match body.collect().await {
        Ok(body) => body.to_bytes(),
        Err(_) => return json_response(StatusCode::BAD_REQUEST, json!({"error": "unable to read request body"})),
    };
    match (method, path.as_str()) {
        (Method::POST, "/mcp") => handle_streamable_post(body, state).await,
        (Method::GET, "/mcp") => streamable_sse_response(),
        (Method::DELETE, "/mcp") => handle_streamable_delete(session_id.as_deref(), state).await,
        (Method::GET, "/sse") => legacy_sse_response(state).await,
        (Method::POST, "/message") => handle_legacy_message(query.as_deref(), body, state).await,
        (Method::POST, _) | (Method::GET, _) => json_response(StatusCode::NOT_FOUND, json!({"error": "mock MCP endpoint not found"})),
        _ => json_response(StatusCode::METHOD_NOT_ALLOWED, json!({"error": "mock MCP method is not supported"})),
    }
}

/// Returns an SSE event stream for a Streamable HTTP GET request.
fn streamable_sse_response() -> Response<MockBody> {
    sse_response(vec![
        "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\",\"params\":{\"level\":\"info\",\"data\":\"mock stream opened\"}}\n\n".to_string(),
        ": keepalive\n\n".to_string(),
    ])
}

/// Deletes a Streamable HTTP session only when the client supplied a known session ID.
async fn handle_streamable_delete(session_id: Option<&str>, state: MockState) -> Response<MockBody> {
    let Some(session_id) = session_id else {
        return json_response(StatusCode::BAD_REQUEST, json!({"error": "Mcp-Session-Id header is required"}));
    };
    if state.remove_streamable_session(session_id).await {
        empty_response(StatusCode::NO_CONTENT)
    } else {
        json_response(StatusCode::NOT_FOUND, json!({"error": "unknown Streamable HTTP session"}))
    }
}

/// Handles a Streamable HTTP JSON-RPC POST.
async fn handle_streamable_post(body: Bytes, state: MockState) -> Response<MockBody> {
    let request = match serde_json::from_slice::<Value>(&body) {
        Ok(request) => request,
        Err(_) => return json_response(StatusCode::BAD_REQUEST, json!({"error": "invalid JSON-RPC body"})),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    match request.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            let session_id = state.open_streamable_session().await;
            let result = json_rpc_result(
                id,
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "spacegate-mcp-mock", "version": "0.1.0"}
                }),
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .header("Mcp-Session-Id", session_id)
                .body(full_body(result))
                .expect("mock initialize response is valid")
        }
        Some("tools/list") => json_response(StatusCode::OK, json_rpc_value(id, tools_result())),
        Some("tools/call") if request.pointer("/params/stream").and_then(Value::as_bool) == Some(true) => {
            let first = json_rpc_result(id, tool_call_result());
            sse_response(vec![format!("data: {first}\n\n"), "data: [DONE]\n\n".to_string()])
        }
        Some("tools/call") => json_response(StatusCode::OK, json_rpc_value(id, tool_call_result())),
        _ => json_response(
            StatusCode::OK,
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "mock method not found"}}),
        ),
    }
}

/// Creates the long-lived Legacy SSE response and sends its endpoint event first.
async fn legacy_sse_response(state: MockState) -> Response<MockBody> {
    let session = state.open_legacy_session().await;
    let mut events = session.into_events();
    let stream = stream::poll_fn(move |context| events.poll_recv(context).map(|event| event.map(|event| Ok::<_, Infallible>(Frame::data(Bytes::from(event))))));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(StreamBody::new(stream).boxed_unsync())
        .expect("mock Legacy SSE response is valid")
}

/// Sends a Legacy SSE JSON-RPC result through the connection identified by the endpoint URL.
async fn handle_legacy_message(query: Option<&str>, body: Bytes, state: MockState) -> Response<MockBody> {
    let Some(session_id) = query.and_then(session_id_from_query) else {
        return json_response(StatusCode::BAD_REQUEST, json!({"error": "sessionId query parameter is required"}));
    };
    let request = match serde_json::from_slice::<Value>(&body) {
        Ok(request) => request,
        Err(_) => return json_response(StatusCode::BAD_REQUEST, json!({"error": "invalid JSON-RPC body"})),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let result = match request.get("method").and_then(Value::as_str) {
        Some("tools/list") => tools_result(),
        Some("tools/call") => tool_call_result(),
        _ => json!({"error": "mock method not found"}),
    };
    if state.send_legacy_response(session_id, json_rpc_result(id, result)).await {
        empty_response(StatusCode::ACCEPTED)
    } else {
        json_response(StatusCode::NOT_FOUND, json!({"error": "unknown Legacy SSE session"}))
    }
}

/// Extracts the deterministic session ID from the Legacy SSE endpoint query string.
fn session_id_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|part| part.strip_prefix("sessionId=")).filter(|session_id| !session_id.is_empty())
}

/// Builds the fixed tools/list payload returned by both mock transports.
fn tools_result() -> Value {
    json!({"tools": [{"name": "mock.echo", "description": "Returns the supplied input", "inputSchema": {"type": "object"}}]})
}

/// Builds the fixed tools/call payload returned by both mock transports.
fn tool_call_result() -> Value {
    json!({"content": [{"type": "text", "text": "mock first chunk"}]})
}

/// Wraps an MCP result object with the request's JSON-RPC envelope.
fn json_rpc_value(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Serializes a JSON-RPC result without changing its field ordering in mock SSE fixtures.
fn json_rpc_result(id: Value, result: Value) -> String {
    json_rpc_value(id, result).to_string()
}

/// Builds a finite JSON response body.
fn json_response(status: StatusCode, body: Value) -> Response<MockBody> {
    Response::builder().status(status).header(header::CONTENT_TYPE, "application/json").body(full_body(body.to_string())).expect("mock JSON response is valid")
}

/// Builds an empty HTTP response body.
fn empty_response(status: StatusCode) -> Response<MockBody> {
    Response::builder().status(status).body(full_body(Bytes::new())).expect("mock empty response is valid")
}

/// Converts finite bytes into the common mock response body type.
fn full_body(body: impl Into<Bytes>) -> MockBody {
    Full::new(body.into()).boxed_unsync()
}

/// Builds a multi-frame SSE response from complete event frames.
fn sse_response(events: Vec<String>) -> Response<MockBody> {
    let frames = events.into_iter().map(|event| Ok::<_, Infallible>(Frame::data(Bytes::from(event))));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(StreamBody::new(stream::iter(frames)).boxed_unsync())
        .expect("mock Streamable HTTP SSE response is valid")
}
