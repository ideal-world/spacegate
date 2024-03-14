// use criterion::{black_box, criterion_group, criterion_main, Criterion};
// use spacegate_plugin::plugins::status::sliding_window::SlidingWindowCounter;
// use spacegate_shell::cache_client::{get as get_cache, init as init_cache};
// use tardis::chrono::{Duration, Utc};
// use tardis::test::test_container::TardisTestContainer;
// use tardis::testcontainers;
// use tardis::tokio::runtime::Runtime;
// use tardis::tokio::time::Instant;

// async fn add_one() -> u64 {
//     let test = SlidingWindowCounter::new(Duration::seconds(60), "");
//     let client = get_cache("test_gate").await.unwrap();
//     test.add_and_count(Utc::now(), client).await.unwrap()
// }

// fn bench(c: &mut Criterion) {
//     c.bench_function("iter", move |b| {
//         b.to_async(Runtime::new().unwrap()).iter_custom(|iters| async move {
//             let docker = testcontainers::clients::Cli::default();
//             let redis_container = TardisTestContainer::redis_custom(&docker);
//             let port = redis_container.get_host_port_ipv4(6379);
//             let url = format!("redis://127.0.0.1:{port}/0",);
//             init_cache("test_gate", &url).await.unwrap();

//             let start = Instant::now();
//             for _i in 0..iters {
//                 black_box(add_one()).await;
//             }
//             start.elapsed()
//         })
//     });
// }

// criterion_group!(benches, bench);
// criterion_main!(benches);
