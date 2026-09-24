use std::convert::Infallible;
use std::net::SocketAddr;

use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use mcp_mock_server::{handle_request, MockState};

/// Starts the local MCP mock server on the requested address.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = listen_address()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    let state = MockState::new();
    eprintln!("MCP mock listening on http://{address}");

    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let state = state.clone();
                async move { Ok::<_, Infallible>(handle_request(request, state).await) }
            });
            if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await {
                eprintln!("MCP mock connection error: {error}");
            }
        });
    }
}

/// Reads an optional `--addr <IP:PORT>` argument for local or test-network use.
fn listen_address() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        None => Ok("127.0.0.1:3001".parse()?),
        Some("--addr") => arguments.next().ok_or("--addr requires an IP:PORT value")?.parse().map_err(Into::into),
        Some("--help") | Some("-h") => {
            println!("Usage: mcp-mock-server [--addr IP:PORT]");
            std::process::exit(0);
        }
        Some(argument) => Err(format!("unknown argument: {argument}").into()),
    }
}
