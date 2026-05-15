//! Covers wasm module loading concerns that sit below Proxy-Wasm execution:
//! remote fetch, digest verification, and cache invalidation.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use spacegate_plugin_wasm::config::WasmPluginShellConfig;
use spacegate_plugin_wasm::fetch::fetch_wasm_bytes_sync;
use spacegate_plugin_wasm::runtime::WasmModuleCache;
use tokio::net::TcpListener;

async fn start_bytes_server(body: Bytes) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let body = body.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |_req: Request<hyper::body::Incoming>| {
                    let body = body.clone();
                    async move { Ok::<_, Infallible>(Response::new(Full::new(body))) }
                });
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
            });
        }
    });
    addr
}

async fn start_oci_registry_server(wasm: Bytes) -> SocketAddr {
    let digest = sha256_hex(&wasm);
    let manifest = Bytes::from(format!(
        r#"{{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.manifest.v1+json",
  "config": {{"mediaType": "application/vnd.unknown.config.v1+json", "digest": "sha256:{}", "size": 2}},
  "layers": [
    {{"mediaType": "application/vnd.module.wasm.content.layer.v1+wasm", "digest": "sha256:{digest}", "size": {}}}
  ]
}}"#,
        "0".repeat(64),
        wasm.len()
    ));
    let mut routes = HashMap::new();
    routes.insert("/v2/plugin/manifests/v1".to_string(), manifest);
    routes.insert(format!("/v2/plugin/blobs/sha256:{digest}"), wasm);
    let routes = Arc::new(routes);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let routes = routes.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let routes = routes.clone();
                    async move {
                        let path = req.uri().path().to_string();
                        if let Some(body) = routes.get(&path) {
                            Ok::<_, Infallible>(Response::new(Full::new(body.clone())))
                        } else {
                            let mut resp = Response::new(Full::new(Bytes::from_static(b"not found")));
                            *resp.status_mut() = StatusCode::NOT_FOUND;
                            Ok(resp)
                        }
                    }
                });
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
            });
        }
    });
    addr
}

fn guest_manifest_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("on_tick_guest");
    p.push("Cargo.toml");
    p
}

fn guest_wasm_path() -> PathBuf {
    let manifest = guest_manifest_path();
    let out = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("cargo metadata: spawn");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse cargo metadata json");
    let target_dir = meta["target_directory"].as_str().expect("target_directory missing");
    PathBuf::from(target_dir).join("wasm32-wasip1").join("release").join("on_tick_guest.wasm")
}

fn ensure_guest_built() -> PathBuf {
    let wasm = guest_wasm_path();
    if !wasm.exists() {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "--release", "--target", "wasm32-wasip1", "--manifest-path"])
            .arg(guest_manifest_path())
            .status()
            .expect("cargo build: spawn");
        assert!(status.success(), "on_tick_guest build failed");
        assert!(wasm.exists(), "wasm still missing after build: {wasm:?}");
    }
    wasm
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[tokio::test]
async fn fetch_wasm_bytes_supports_http_urls() {
    let expected = Bytes::from_static(b"hello wasm over http");
    let addr = start_bytes_server(expected.clone()).await;
    let url = format!("http://{addr}/plugin.wasm");

    let fetched = tokio::task::spawn_blocking(move || fetch_wasm_bytes_sync(&url)).await.expect("join").expect("fetch");

    assert_eq!(fetched, expected);
}

#[tokio::test]
async fn fetch_wasm_bytes_supports_oci_image_layers() {
    let expected = Bytes::from_static(b"\0asm\x01\0\0\0");
    let addr = start_oci_registry_server(expected.clone()).await;
    let url = format!("oci://{addr}/plugin:v1");

    let fetched = tokio::task::spawn_blocking(move || fetch_wasm_bytes_sync(&url)).await.expect("join").expect("fetch");

    assert_eq!(fetched, expected);
}

#[test]
fn wasm_module_cache_uses_module_cache_key_for_invalidation() {
    let wasm = ensure_guest_built();
    let bytes = std::fs::read(&wasm).expect("read wasm");
    let sha256 = sha256_hex(&bytes);
    let cache = WasmModuleCache::new(8);

    let cfg_v1 = WasmPluginShellConfig {
        url: format!("file://{}", wasm.display()),
        sha256: Some(sha256.clone()),
        module_cache_key: Some("on-tick:v1".to_string()),
        ..Default::default()
    };
    let first = cache.get_or_compile(&cfg_v1).expect("compile v1");
    let cached = cache.get_or_compile(&cfg_v1).expect("compile v1 cached");
    assert!(Arc::ptr_eq(&first, &cached));

    let cfg_v2 = WasmPluginShellConfig {
        module_cache_key: Some("on-tick:v2".to_string()),
        ..cfg_v1
    };
    let second = cache.get_or_compile(&cfg_v2).expect("compile v2");
    assert!(!Arc::ptr_eq(&first, &second));
}

#[test]
fn wasm_module_cache_rejects_sha256_mismatch() {
    let wasm = ensure_guest_built();
    let cache = WasmModuleCache::new(8);
    let cfg = WasmPluginShellConfig {
        url: format!("file://{}", wasm.display()),
        sha256: Some("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()),
        use_cache: false,
        ..Default::default()
    };

    let err = cache.get_or_compile(&cfg).expect_err("expected sha mismatch");
    assert!(err.to_string().contains("sha256 mismatch"), "{err}");
}
