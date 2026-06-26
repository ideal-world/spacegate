//! 同步拉取 WASM 字节（在 `Plugin::create` 同步上下文中使用）。
//!
//! 支持：`file://...`、裸文件系统路径、`http(s)://...` 与 OCI 镜像 URL。
//! 网络拉取通过临时线程运行 async reqwest，避免在 `Plugin::create` 这条同步路径里嵌套 tokio runtime。

use crate::config::OciAuthConfig;
use crate::error::WasmHostError;
use flate2::read::GzDecoder;
use reqwest::header::{ACCEPT, WWW_AUTHENTICATE};
use serde::Deserialize;
use std::{collections::HashMap, io::Read, time::Duration};
use tar::Archive;

const OCI_MANIFEST_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.artifact.manifest.v1+json";
const OCI_BLOB_ACCEPT: &str = "application/vnd.module.wasm.content.layer.v1+wasm, application/wasm, application/vnd.wasm.content.layer.v1+wasm, application/octet-stream, application/vnd.docker.image.rootfs.diff.tar.gzip, application/vnd.oci.image.layer.v1.tar+gzip";

fn fetch_http_wasm_bytes_sync(url: &str) -> Result<Vec<u8>, WasmHostError> {
    let url = url.to_string();
    std::thread::Builder::new()
        .name("spacegate-wasm-fetch".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| WasmHostError::Fetch(format!("build fetch runtime: {e}")))?;
            rt.block_on(async move {
                let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().map_err(|e| WasmHostError::Fetch(format!("build http client: {e}")))?;
                let resp = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| WasmHostError::Fetch(format!("GET {url}: {e}")))?
                    .error_for_status()
                    .map_err(|e| WasmHostError::Fetch(format!("GET {url}: {e}")))?;
                let bytes = resp.bytes().await.map_err(|e| WasmHostError::Fetch(format!("read {url} body: {e}")))?;
                Ok(bytes.to_vec())
            })
        })
        .map_err(|e| WasmHostError::Fetch(format!("spawn fetch thread: {e}")))?
        .join()
        .map_err(|_| WasmHostError::Fetch("fetch thread panicked".to_string()))?
}

fn fetch_oci_wasm_bytes_sync(url: &str, auth: Option<OciAuthConfig>) -> Result<Vec<u8>, WasmHostError> {
    let url = url.to_string();
    std::thread::Builder::new()
        .name("spacegate-wasm-oci-fetch".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| WasmHostError::Fetch(format!("build OCI fetch runtime: {e}")))?;
            rt.block_on(async move {
                let reference = OciReference::parse(&url)?;
                if let Some(auth_registry) = auth.as_ref().and_then(|a| a.registry.as_deref()).filter(|v| !v.trim().is_empty()) {
                    if !auth_registry.eq_ignore_ascii_case(&reference.registry) {
                        return Err(WasmHostError::Fetch(format!(
                            "OCI auth registry `{auth_registry}` does not match image registry `{}`",
                            reference.registry
                        )));
                    }
                }
                let client = reqwest::Client::builder().timeout(Duration::from_secs(60)).build().map_err(|e| WasmHostError::Fetch(format!("build OCI client: {e}")))?;
                fetch_oci_wasm_bytes(&client, &reference, auth.as_ref()).await
            })
        })
        .map_err(|e| WasmHostError::Fetch(format!("spawn OCI fetch thread: {e}")))?
        .join()
        .map_err(|_| WasmHostError::Fetch("OCI fetch thread panicked".to_string()))?
}

pub fn fetch_wasm_bytes_sync(url_or_path: &str) -> Result<Vec<u8>, WasmHostError> {
    fetch_wasm_bytes_sync_with_auth(url_or_path, None)
}

pub fn fetch_wasm_bytes_sync_with_auth(url_or_path: &str, oci_auth: Option<&OciAuthConfig>) -> Result<Vec<u8>, WasmHostError> {
    let trim = url_or_path.trim();
    if let Some(rest) = trim.strip_prefix("file://") {
        return std::fs::read(rest).map_err(|e| WasmHostError::Fetch(format!("read file {rest}: {e}")));
    }
    if trim.starts_with("http://") || trim.starts_with("https://") {
        return fetch_http_wasm_bytes_sync(trim);
    }
    if is_oci_url(trim) {
        return fetch_oci_wasm_bytes_sync(trim, oci_auth.cloned());
    }
    std::fs::read(trim).map_err(|e| WasmHostError::Fetch(format!("read path {trim}: {e}")))
}

pub fn is_oci_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("oci://") || lower.starts_with("docker://") || lower.starts_with("image://") || lower.starts_with("oci+http://")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OciReference {
    scheme: &'static str,
    registry: String,
    repository: String,
    reference: String,
}

impl OciReference {
    fn parse(url: &str) -> Result<Self, WasmHostError> {
        let trim = url.trim();
        let (scheme, rest) = if let Some(rest) = trim.strip_prefix("oci+http://") {
            ("http", rest)
        } else if let Some(rest) = trim.strip_prefix("oci://") {
            (default_oci_scheme(rest), rest)
        } else if let Some(rest) = trim.strip_prefix("docker://") {
            (default_oci_scheme(rest), rest)
        } else if let Some(rest) = trim.strip_prefix("image://") {
            (default_oci_scheme(rest), rest)
        } else {
            return Err(WasmHostError::Fetch(format!("unsupported OCI URL scheme: {trim}")));
        };

        let Some((registry, image)) = rest.split_once('/') else {
            return Err(WasmHostError::Fetch(format!("OCI URL must include registry and repository: {trim}")));
        };
        if registry.trim().is_empty() || image.trim().is_empty() {
            return Err(WasmHostError::Fetch(format!("OCI URL must include registry and repository: {trim}")));
        }

        let (repository, reference) = if let Some((repository, digest)) = image.rsplit_once('@') {
            (repository, digest)
        } else if let Some((repository, tag)) = split_tag(image) {
            (repository, tag)
        } else {
            (image, "latest")
        };
        if repository.trim().is_empty() || reference.trim().is_empty() {
            return Err(WasmHostError::Fetch(format!("OCI URL must include repository and tag/digest: {trim}")));
        }

        Ok(Self {
            scheme,
            registry: registry.to_string(),
            repository: repository.to_string(),
            reference: reference.to_string(),
        })
    }

    fn manifest_url(&self, reference: &str) -> String {
        format!("{}://{}/v2/{}/manifests/{}", self.scheme, self.registry, self.repository, reference)
    }

    fn blob_url(&self, digest: &str) -> String {
        format!("{}://{}/v2/{}/blobs/{}", self.scheme, self.registry, self.repository, digest)
    }
}

fn default_oci_scheme(rest: &str) -> &'static str {
    let registry = rest.split('/').next().unwrap_or_default();
    if registry.starts_with("localhost")
        || registry.starts_with("127.0.0.1")
        || registry.starts_with("[::1]")
        || registry.starts_with("host.docker.internal")
    {
        "http"
    } else {
        "https"
    }
}

fn split_tag(image: &str) -> Option<(&str, &str)> {
    let slash = image.rfind('/').map(|idx| idx + 1).unwrap_or(0);
    let colon = image[slash..].rfind(':').map(|idx| slash + idx)?;
    Some((&image[..colon], &image[colon + 1..]))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OciManifest {
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    manifests: Vec<OciDescriptor>,
    #[serde(default)]
    layers: Vec<OciDescriptor>,
    #[serde(default)]
    blobs: Vec<OciDescriptor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OciDescriptor {
    media_type: String,
    digest: String,
    #[serde(default)]
    platform: Option<OciPlatform>,
}

#[derive(Debug, Deserialize)]
struct OciPlatform {
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    os: Option<String>,
}

async fn fetch_oci_wasm_bytes(client: &reqwest::Client, reference: &OciReference, auth: Option<&OciAuthConfig>) -> Result<Vec<u8>, WasmHostError> {
    let manifest = fetch_oci_manifest(client, reference, &reference.reference, auth).await?;
    let manifest = if is_index_manifest(&manifest) {
        let child = select_manifest_descriptor(&manifest.manifests)?;
        fetch_oci_manifest(client, reference, &child.digest, auth).await?
    } else {
        manifest
    };
    // 1. 优先尝试原生 WASM artifact 层
    if let Ok(layer) = select_wasm_descriptor(&manifest) {
        return registry_get_bytes(client, &reference.blob_url(&layer.digest), OCI_BLOB_ACCEPT, reference, auth).await;
    }
    // 2. 回退：Docker 镜像格式（tar.gz 层内包含 .wasm 文件）
    let docker_layers: Vec<_> = manifest.layers.iter().filter(|l| is_docker_tar_media_type(&l.media_type)).collect();
    if !docker_layers.is_empty() {
        // 从最后一层（最上层）向前搜索，Docker 镜像最后一层包含最终文件系统
        for layer in docker_layers.iter().rev() {
            let blob = registry_get_bytes(client, &reference.blob_url(&layer.digest), OCI_BLOB_ACCEPT, reference, auth).await?;
            if let Ok(wasm_bytes) = extract_wasm_from_docker_layer(&blob) {
                return Ok(wasm_bytes);
            }
        }
        return Err(WasmHostError::Fetch(
            "Docker image layers do not contain a .wasm file; ensure the image includes a .wasm file in its filesystem".to_string(),
        ));
    }
    Err(WasmHostError::Fetch("OCI image does not contain a wasm layer".to_string()))
}

async fn fetch_oci_manifest(client: &reqwest::Client, reference: &OciReference, manifest_ref: &str, auth: Option<&OciAuthConfig>) -> Result<OciManifest, WasmHostError> {
    let url = reference.manifest_url(manifest_ref);
    let bytes = registry_get_bytes(client, &url, OCI_MANIFEST_ACCEPT, reference, auth).await?;
    serde_json::from_slice(&bytes).map_err(|e| WasmHostError::Fetch(format!("parse OCI manifest {url}: {e}")))
}

async fn registry_get_bytes(client: &reqwest::Client, url: &str, accept: &str, reference: &OciReference, auth: Option<&OciAuthConfig>) -> Result<Vec<u8>, WasmHostError> {
    let send = |token: Option<&str>| {
        let req = client.get(url).header(ACCEPT, accept);
        apply_registry_auth(req, auth, token)
    };

    let resp = send(None).send().await.map_err(|e| WasmHostError::Fetch(format!("GET {url}: {e}")))?;
    let resp = if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let challenge = resp.headers().get(WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()).unwrap_or_default();
        let token = fetch_bearer_token(client, challenge, reference, auth).await?;
        send(Some(&token)).send().await.map_err(|e| WasmHostError::Fetch(format!("GET {url}: {e}")))?
    } else {
        resp
    };

    let status = resp.status();
    if !status.is_success() {
        return Err(WasmHostError::Fetch(format!("GET {url}: {status}")));
    }
    let bytes = resp.bytes().await.map_err(|e| WasmHostError::Fetch(format!("read OCI response {url}: {e}")))?;
    Ok(bytes.to_vec())
}

fn apply_registry_auth(req: reqwest::RequestBuilder, auth: Option<&OciAuthConfig>, bearer_token: Option<&str>) -> reqwest::RequestBuilder {
    if let Some(token) = bearer_token {
        return req.bearer_auth(token);
    }
    let Some(auth) = auth else {
        return req;
    };
    if let Some(token) = auth.bearer_token.as_deref().or(auth.identity_token.as_deref()).filter(|v| !v.trim().is_empty()) {
        return req.bearer_auth(token);
    }
    if let Some(username) = auth.username.as_deref().filter(|v| !v.trim().is_empty()) {
        return req.basic_auth(username, auth.password.clone());
    }
    req
}

async fn fetch_bearer_token(client: &reqwest::Client, challenge: &str, reference: &OciReference, auth: Option<&OciAuthConfig>) -> Result<String, WasmHostError> {
    let params =
        parse_bearer_challenge(challenge).ok_or_else(|| WasmHostError::Fetch(format!("registry {} requires auth but did not return a Bearer challenge", reference.registry)))?;
    let realm = params.get("realm").filter(|v| !v.trim().is_empty()).ok_or_else(|| WasmHostError::Fetch("Bearer auth challenge missing realm".to_string()))?;
    let mut url = reqwest::Url::parse(realm).map_err(|e| WasmHostError::Fetch(format!("parse Bearer token realm {realm}: {e}")))?;
    {
        let mut query = url.query_pairs_mut();
        if let Some(service) = params.get("service").filter(|v| !v.trim().is_empty()) {
            query.append_pair("service", service);
        }
        let scope = params.get("scope").cloned().unwrap_or_else(|| format!("repository:{}:pull", reference.repository));
        query.append_pair("scope", &scope);
    }

    let req = apply_registry_auth(client.get(url.clone()), auth, None);
    let resp = req.send().await.map_err(|e| WasmHostError::Fetch(format!("GET OCI token {url}: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(WasmHostError::Fetch(format!("GET OCI token {url}: {status}")));
    }
    let bytes = resp.bytes().await.map_err(|e| WasmHostError::Fetch(format!("read OCI token {url}: {e}")))?;
    let token: OciTokenResponse = serde_json::from_slice(&bytes).map_err(|e| WasmHostError::Fetch(format!("parse OCI token response {url}: {e}")))?;
    token.token.or(token.access_token).filter(|v| !v.trim().is_empty()).ok_or_else(|| WasmHostError::Fetch(format!("OCI token response {url} did not include token")))
}

#[derive(Debug, Deserialize)]
struct OciTokenResponse {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
}

fn parse_bearer_challenge(header: &str) -> Option<HashMap<String, String>> {
    let rest = header.trim().strip_prefix("Bearer ")?;
    let mut params = HashMap::new();
    for part in split_quoted_commas(rest) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        params.insert(key.trim().to_ascii_lowercase(), value.trim().trim_matches('"').to_string());
    }
    Some(params)
}

fn split_quoted_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    for (idx, ch) in value.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                parts.push(value[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

fn is_index_manifest(manifest: &OciManifest) -> bool {
    manifest.media_type.as_deref().map(|mt| mt.contains("image.index") || mt.contains("manifest.list")).unwrap_or(false) || !manifest.manifests.is_empty()
}

fn select_manifest_descriptor(manifests: &[OciDescriptor]) -> Result<&OciDescriptor, WasmHostError> {
    manifests
        .iter()
        .find(|m| m.platform.as_ref().map(|p| p.architecture.as_deref() == Some("wasm") || p.os.as_deref() == Some("wasi")).unwrap_or(false))
        .or_else(|| manifests.first())
        .ok_or_else(|| WasmHostError::Fetch("OCI image index does not contain manifests".to_string()))
}

fn select_wasm_descriptor(manifest: &OciManifest) -> Result<&OciDescriptor, WasmHostError> {
    let descriptors = manifest.layers.iter().chain(manifest.blobs.iter()).collect::<Vec<_>>();
    descriptors
        .iter()
        .copied()
        .find(|layer| is_wasm_media_type(&layer.media_type))
        // Single-layer fallback only when it is NOT a Docker/OCI tar.gz layer
        // (Docker tar.gz layers are handled by the Docker-format extraction path in fetch_oci_wasm_bytes)
        .or_else(|| {
            (descriptors.len() == 1 && !is_docker_tar_media_type(&descriptors[0].media_type))
                .then(|| descriptors[0])
        })
        .ok_or_else(|| WasmHostError::Fetch("OCI image does not contain a wasm layer".to_string()))
}

fn is_wasm_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/vnd.module.wasm.content.layer.v1+wasm" | "application/vnd.wasm.content.layer.v1+wasm" | "application/wasm"
    ) || media_type.contains("wasm")
}

/// 判断是否为 Docker/OCI 镜像的 tar.gz 层（Docker 镜像格式推送的 WASM 插件）
fn is_docker_tar_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/vnd.docker.image.rootfs.diff.tar.gzip"
            | "application/vnd.oci.image.layer.v1.tar+gzip"
            | "application/vnd.oci.image.layer.nondistributable.v1.tar+gzip"
    )
}

/// 从 Docker 镜像的 tar.gz 层中提取第一个 `.wasm` 文件。
fn extract_wasm_from_docker_layer(tar_gz_data: &[u8]) -> Result<Vec<u8>, WasmHostError> {
    let gz = GzDecoder::new(tar_gz_data);
    let mut archive = Archive::new(gz);
    for entry in archive.entries().map_err(|e| WasmHostError::Fetch(format!("read tar.gz entries: {e}")))? {
        let mut entry = entry.map_err(|e| WasmHostError::Fetch(format!("read tar.gz entry: {e}")))?;
        let path = match entry.path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        if path.ends_with(".wasm") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| WasmHostError::Fetch(format!("read .wasm from tar.gz entry {path}: {e}")))?;
            if !buf.is_empty() {
                return Ok(buf);
            }
        }
    }
    Err(WasmHostError::Fetch("no .wasm file found in tar.gz layer".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_oci_reference_tag_digest_and_default_tag() {
        assert_eq!(
            OciReference::parse("oci://registry.example.com/ns/plugin:v1").unwrap(),
            OciReference {
                scheme: "https",
                registry: "registry.example.com".to_string(),
                repository: "ns/plugin".to_string(),
                reference: "v1".to_string(),
            }
        );
        assert_eq!(OciReference::parse("docker://localhost:5000/plugin").unwrap().reference, "latest");
        assert_eq!(OciReference::parse("image://registry.example.com/ns/plugin@sha256:abc").unwrap().reference, "sha256:abc");
    }

    #[test]
    fn parses_bearer_challenge() {
        let parsed = parse_bearer_challenge(r#"Bearer realm="https://auth.example/token",service="registry.example",scope="repository:ns/plugin:pull""#).unwrap();
        assert_eq!(parsed["realm"], "https://auth.example/token");
        assert_eq!(parsed["service"], "registry.example");
        assert_eq!(parsed["scope"], "repository:ns/plugin:pull");
    }

    #[test]
    fn detects_docker_tar_media_types() {
        assert!(is_docker_tar_media_type("application/vnd.docker.image.rootfs.diff.tar.gzip"));
        assert!(is_docker_tar_media_type("application/vnd.oci.image.layer.v1.tar+gzip"));
        assert!(!is_docker_tar_media_type("application/vnd.module.wasm.content.layer.v1+wasm"));
        assert!(!is_docker_tar_media_type("application/octet-stream"));
    }

    #[test]
    fn extracts_wasm_from_tar_gz_layer() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let wasm_content = b"(module)";
        let mut tar_builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("plugin.wasm").unwrap();
        header.set_size(wasm_content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar_builder.append(&header, &wasm_content[..]).unwrap();
        let tar_bytes = tar_builder.into_inner().unwrap();

        let mut gz_buf = Vec::new();
        let mut encoder = GzEncoder::new(&mut gz_buf, Compression::fast());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap();

        let extracted = extract_wasm_from_docker_layer(&gz_buf).unwrap();
        assert_eq!(extracted, wasm_content);
    }

    #[test]
    fn extract_wasm_from_tar_gz_layer_no_wasm() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let content = b"hello world";
        let mut tar_builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("readme.txt").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar_builder.append(&header, &content[..]).unwrap();
        let tar_bytes = tar_builder.into_inner().unwrap();

        let mut gz_buf = Vec::new();
        let mut encoder = GzEncoder::new(&mut gz_buf, Compression::fast());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap();

        let result = extract_wasm_from_docker_layer(&gz_buf);
        assert!(result.is_err());
    }

    #[test]
    fn select_wasm_descriptor_skips_docker_tar_single_layer() {
        // A Docker-format image has a single tar.gz layer;
        // select_wasm_descriptor should NOT select it (the Docker extraction path handles it).
        let manifest = OciManifest {
            media_type: Some("application/vnd.docker.distribution.manifest.v2+json".to_string()),
            manifests: vec![],
            layers: vec![OciDescriptor {
                media_type: "application/vnd.docker.image.rootfs.diff.tar.gzip".to_string(),
                digest: "sha256:abc".to_string(),
                platform: None,
            }],
            blobs: vec![],
        };
        assert!(select_wasm_descriptor(&manifest).is_err());
    }

    #[test]
    fn select_wasm_descriptor_picks_native_wasm_layer() {
        let manifest = OciManifest {
            media_type: None,
            manifests: vec![],
            layers: vec![OciDescriptor {
                media_type: "application/vnd.module.wasm.content.layer.v1+wasm".to_string(),
                digest: "sha256:wasm".to_string(),
                platform: None,
            }],
            blobs: vec![],
        };
        let desc = select_wasm_descriptor(&manifest).unwrap();
        assert_eq!(desc.digest, "sha256:wasm");
    }

    #[test]
    fn select_wasm_descriptor_single_non_docker_layer_fallback() {
        // A single layer with an unknown (but non-Docker) media type should still be selected
        let manifest = OciManifest {
            media_type: None,
            manifests: vec![],
            layers: vec![OciDescriptor {
                media_type: "application/octet-stream".to_string(),
                digest: "sha256:fallback".to_string(),
                platform: None,
            }],
            blobs: vec![],
        };
        let desc = select_wasm_descriptor(&manifest).unwrap();
        assert_eq!(desc.digest, "sha256:fallback");
    }
}
