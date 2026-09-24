use std::time::Duration;

use mcp_mock_server::MockState;

/// A Legacy SSE request must deliver its result through the connection created by GET /sse.
#[tokio::test]
async fn legacy_message_is_delivered_to_the_original_sse_connection() {
    let mock = MockState::new();
    let mut session = mock.open_legacy_session().await;

    assert_eq!(session.next_event().await.as_deref(), Some("event: endpoint\ndata: /message?sessionId=legacy-1\n\n"));

    assert!(mock.send_legacy_response(&session.id, r#"{"jsonrpc":"2.0","id":7,"result":{"tools":[]}}"#.to_string()).await);

    let response = tokio::time::timeout(Duration::from_millis(100), session.next_event()).await.expect("Legacy SSE response must arrive on the already-open connection");
    assert_eq!(
        response.as_deref(),
        Some("event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"tools\":[]}}\n\n")
    );
}
