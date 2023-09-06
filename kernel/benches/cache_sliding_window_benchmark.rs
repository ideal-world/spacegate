use criterion::{black_box, criterion_group, criterion_main, Criterion};
use http::{HeaderMap, Method, Uri, Version};
use hyper::Body;
use spacegate_kernel::functions::cache_client;
use spacegate_kernel::plugins::context::SgRoutePluginContext;
use spacegate_kernel::plugins::filters::status::sliding_window::SlidingWindowCounter;
use tardis::chrono::{Duration, Utc};
use tardis::futures::executor::block_on;
use tardis::test::test_container::TardisTestContainer;
use tardis::testcontainers;
use tardis::tokio::runtime::Runtime;
use tardis::tokio::time::Instant;

async fn add_one() -> u64 {
    let test = SlidingWindowCounter::new(Duration::seconds(60), "");
    test.add_and_count(
        Utc::now(),
        &SgRoutePluginContext::new_http(
            Method::GET,
            Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "test_gate".to_string(),
            None,
        ),
    )
    .await
    .unwrap()
}

fn bench(c: &mut Criterion) {
    c.bench_function("iter", move |b| {
        b.to_async(Runtime::new().unwrap()).iter_custom(|iters| async move {
            let docker = testcontainers::clients::Cli::default();
            let redis_container = TardisTestContainer::redis_custom(&docker);
            let port = redis_container.get_host_port_ipv4(6379);
            let url = format!("redis://127.0.0.1:{port}/0",);
            cache_client::init("test_gate", &url).await.unwrap();

            let start = Instant::now();
            for _i in 0..iters {
                black_box(add_one()).await;
            }
            start.elapsed()
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
