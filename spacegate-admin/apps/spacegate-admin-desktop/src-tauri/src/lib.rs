use std::collections::HashMap;
use tauri::utils::config::{Csp, CspDirectiveSources};

/// 校验运行期 API 地址；例如 `https://api.example.test/api/v1`，不接收认证信息或查询参数。
fn parse_runtime_api(value: &str) -> Result<tauri::Url, &'static str> {
    let url = tauri::Url::parse(value).map_err(|_| "Invalid PUBLIC_API_BASE")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "PUBLIC_API_BASE must be an HTTP(S) URL without credentials, query or fragment",
        );
    }
    Ok(url)
}

/// 仅将当前运行期 API 的 origin 加入 CSP；不放宽到所有 HTTP/HTTPS 站点。
fn runtime_csp(policy: Csp, origin: &str) -> Csp {
    let mut directives: HashMap<String, CspDirectiveSources> = policy.into();
    for name in ["connect-src", "img-src"] {
        directives.entry(name.to_string()).or_default().push(origin);
    }
    directives.into()
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Desktop.", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 构建不读取部署环境；启动时从进程 PUBLIC_API_BASE 获取公开地址。
    let api = parse_runtime_api(
        &std::env::var("PUBLIC_API_BASE")
            .unwrap_or_else(|_| "http://localhost:3000/api/v1".to_string()),
    )
    .expect("Invalid runtime API configuration");
    let mut context = tauri::generate_context!();
    if let Some(policy) = context.config_mut().app.security.csp.take() {
        context.config_mut().app.security.csp =
            Some(runtime_csp(policy, &api.origin().ascii_serialization()));
    }
    let script = format!(
        "globalThis.__HAI_PUBLIC_CONFIG__={};",
        serde_json::json!({ "apiBase": api.as_str() })
    );
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .append_invoke_initialization_script(script)
        .invoke_handler(tauri::generate_handler![greet])
        .run(context)
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_runtime_address_and_scopes_csp() {
        let api = parse_runtime_api("https://api.example.test:8443/custom/v2").unwrap();
        let policy = runtime_csp(
            Csp::Policy("default-src 'self'; connect-src 'self'".into()),
            &api.origin().ascii_serialization(),
        );
        let directives: HashMap<String, CspDirectiveSources> = policy.into();
        let sources: Vec<String> = directives["connect-src"].clone().into();
        assert!(sources.contains(&"https://api.example.test:8443".to_string()));
        assert!(!sources.contains(&"https:".to_string()));
        for value in [
            "file:///tmp/config",
            "https://user:secret@host",
            "https://host/?token=secret",
            "relative",
        ] {
            assert!(parse_runtime_api(value).is_err());
        }
    }
}
