use std::net::SocketAddr;

use hyper::{service::HttpService, Request};
use serde_json::json;
use spacegate_kernel::{extension::PeerAddr, service::http_route::HttpBackend, BoxError, SgBody};
use spacegate_model::{PluginConfig, PluginInstanceId, PluginInstanceName};
use spacegate_plugin::{mount::MountPointIndex, PluginRepository};

#[tokio::test]
async fn test_hot_update() -> Result<(), BoxError> {
    let mock_addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let repo = PluginRepository::global();
    let id = PluginInstanceId::new("maintenance", PluginInstanceName::named("name"));
    repo.create_or_update_instance(PluginConfig {
        id: id.clone(),
        spec: json!(
            {
                "code": "maintenance",
                "msg": "hello world",
                "body": {
                    "kind": "Json",
                    "value": {"message": "hello world"}
                }
            }
        ),
    })?;
    let mut backend = HttpBackend::builder().build();
    repo.mount(
        &mut backend,
        MountPointIndex::HttpBackend {
            gateway: "".into(),
            route: "".into(),
            rule: 0,
            backend: 0,
        },
        id.clone(),
    )
    .expect("success to mount plugin");
    let mut svc = backend.as_service();
    let req = Request::get("/").extension(PeerAddr(mock_addr)).body(SgBody::empty()).unwrap();
    let resp = svc.call(req).await?;
    assert_eq!(resp.status(), 503);
    let dumped = resp.into_body().dump().await?;
    let body = dumped.get_dumped().expect("body dumped");
    dbg!(body);
    repo.create_or_update_instance(PluginConfig {
        id: id.clone(),
        spec: json!(
            {
                "code": "maintenance",
                "msg": "hello world",
                "body": {
                    "kind": "Json",
                    "value": {"message": "hello world"}
                },
                "redirect": "/redirect",
            }
        ),
    })?;
    let req = Request::get("/").extension(PeerAddr(mock_addr)).body(SgBody::empty()).unwrap();
    let resp = svc.call(req).await?;
    
    assert_eq!(resp.status(), 307);
    let dumped = resp.into_body().dump().await?;
    let body = dumped.get_dumped().expect("body dumped");
    dbg!(body);
    Ok(())
}
