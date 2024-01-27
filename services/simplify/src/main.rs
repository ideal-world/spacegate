use spacegate_kernel::config::{gateway_dto::{SgGateway, SgListener, SgProtocol}, http_route_dto::SgHttpRoute};
use tardis::{basic::tracing::TardisTracing, tokio};

fn main() {
    let tracing = TardisTracing::initializer().with_env_layer().with_fmt_layer().init();
    let conf_path = std::env::args().nth(1).expect("The first parameter is missing: configuration path");
    let check_interval_sec = std::env::args().nth(2).expect("The second parameter is missing: configuration change check period (in seconds)");
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("spacegate").build().expect("fail to build runtime");
    let gateway = SgGateway {
        name: "gateway".into(),
        parameters: Default::default(),
        listeners: vec![
            SgListener { name: Some("default_listener".into()), ip: Default::default(), port: 9001, protocol: SgProtocol::Http, ..Default::default() }
        ],
        filters: None,
    };
    let http_routes = vec![
        SgHttpRoute { gateway_name: "gateway".into(), hostnames: None, filters: None, rules: None }
    ];
    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();
        local_set
            .run_until(async move {
                tokio::task::spawn_local(async move {
                    let server = spacegate_kernel::server::RunningSgGateway::create(gateway, http_routes).expect("fail to start");
                    server.start().await;
                    spacegate_kernel::wait_graceful_shutdown().await.expect("fail to shutdown");
                }).await.unwrap();
            })
            .await;
    });
}
