// use criterion::{criterion_group, criterion_main, Criterion};
// use spacegate_plugin::plugins::status::sliding_window::SlidingWindowCounter;
// use tardis::chrono::{Duration, Utc};
// use tardis::tokio::time::Instant;

// fn bench(c: &mut Criterion) {
//     c.bench_function("iter", move |b| {
//         b.iter_custom(|iters| {
//             let mut test = SlidingWindowCounter::new(Duration::seconds(60), 12);
//             test.init(Utc::now());
//             let start = Instant::now();
//             for _i in 0..iters {
//                 let now = Utc::now();
//                 test.count_in_window(now);
//                 test.add_one(now);
//             }
//             start.elapsed()
//         })
//     });
// }

// criterion_group!(benches, bench);
// criterion_main!(benches);
