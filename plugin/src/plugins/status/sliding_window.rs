use tardis::basic::{error::TardisError, result::TardisResult};
#[cfg(feature = "cache")]
use tardis::cache::Script;
use tardis::chrono::{DateTime, Duration, Utc};
#[cfg(feature = "cache")]
use tardis::tardis_static;

pub(super) const DEFAULT_CONF_WINDOW_KEY: &str = "sg:plugin:filter:window:key";

#[cfg(feature = "cache")]
tardis_static! {
    /// Sliding window script
    ///
    /// # Arguments
    ///
    /// * KEYS[1]  window key
    /// * ARGV[1]  window size
    /// * ARGV[2]  current timestamp
    /// * ARGV[3]  current sub_second microsecond
    ///
    /// # Return
    ///
    /// * count
    ///
    /// # Kernel logic
    ///
    /// -- Extract the key from the KEYS array, which represents the Redis key used for the sorted set.
    /// local key = KEYS[1]
    ///
    /// -- Convert the window size from the ARGV array into a numeric value.
    /// local window_size = tonumber(ARGV[1])
    ///
    /// -- Convert the current time timestamp, including seconds and microseconds, from ARGV into numeric values.
    /// local current_time_timestamp = tonumber(ARGV[2])
    /// local current_time_subsec_micros = tonumber(ARGV[3])
    ///
    /// -- Calculate the member_score, which combines timestamp and microseconds.
    /// local member_score = ((current_time_timestamp % 10000000) * 1000000) + current_time_subsec_micros
    ///
    /// -- Calculate the timestamp when the current window should expire, in milliseconds.
    /// local window_expire_at = (current_time_timestamp * 1000) + window_size
    ///
    /// -- Remove elements from the sorted set that are older than (now - window) based on member_score.
    /// redis.call('ZREMRANGEBYSCORE', key, 0, (member_score - (window_size * 1000)))
    ///
    /// -- Get the number of requests in the current window by counting the elements in the sorted set.
    /// local current_requests_count = redis.call('ZCARD', key)
    ///
    /// -- Add the current request's member_score to the sorted set.
    /// redis.call('ZADD', key, member_score, member_score)
    ///
    /// -- Set the expiration time for the key, specifying when the window should expire.
    /// redis.call('PEXPIRE', key, window_expire_at)
    ///
    /// -- Return the count of requests in the current window.
    /// return current_requests_count
    pub script: Script = Script::new(
        r"
    local key = KEYS[1]

    local window_size = tonumber(ARGV[1])
    local current_time_timestamp = tonumber(ARGV[2])
    local current_time_subsec_micros = tonumber(ARGV[3])

    local member_score = ((current_time_timestamp % 10000000) * 1000000) + current_time_subsec_micros
    local window_expire_at = (current_time_timestamp * 1000) + window_size

    redis.call('ZREMRANGEBYSCORE', key, 0, (member_score - (window_size * 1000)))

    local current_requests_count = redis.call('ZCARD', key)

    redis.call('ZADD', key, member_score, member_score)

    redis.call('PEXPIRE', key, window_expire_at)

    return current_requests_count
    ",
    );
}

/// # SlidingWindowCounter:
///
/// This is a sliding window counter that provides two implementations. When
/// the 'cache features' option is enabled, it uses Redis for storage.
/// When using the cache implementation, it can support the 'status' plugin to
/// run in a distributed manner.
/// The other implementation is memory-based, and it does not support distributed
/// operation via the 'status' plugin.
///
/// ## Performance:
/// - Redis Implementation: Adds and counts operations take close to a milliseconds.
/// - Memory Implementation: Adds and counts operations take nanoseconds.
///
///
/// ## Note:
/// - The Redis-based implementation is suitable for distributed systems
/// and offers higher-level performance with millisecond-level accuracy.
/// - The Memory-based implementation is more efficient in terms of performance
/// but lacks distributed support and offers nanosecond-level accuracy.
#[derive(Debug, Clone)]
pub struct SlidingWindowCounter {
    window_size: Duration,
    #[cfg(not(feature = "cache"))]
    data: Vec<Slot>,
    /// slot_num equal to data.len()
    #[cfg(not(feature = "cache"))]
    slot_num: usize,
    /// milliseconds
    #[cfg(not(feature = "cache"))]
    interval: i64,
    /// range: 0--(slot_num-1)
    #[cfg(not(feature = "cache"))]
    start_slot: usize,
    #[cfg(feature = "cache")]
    window_key: String,
}

impl SlidingWindowCounter {
    #[cfg(feature = "cache")]
    pub fn new(window_size: Duration, window_key: &str) -> Self {
        SlidingWindowCounter {
            window_size,
            window_key: window_key.to_string(),
        }
    }

    #[cfg(not(feature = "cache"))]
    pub fn new(window_size: Duration, _slot_num: usize) -> Self {
        let interval = window_size.num_milliseconds() / _slot_num as i64;
        let mut result = SlidingWindowCounter {
            window_size,
            data: vec![Slot::default(); _slot_num],
            slot_num: _slot_num,
            interval,
            start_slot: 0,
        };
        result.init(Utc::now());
        result
    }

    #[cfg(not(feature = "cache"))]
    /// Initialize the sliding window ,set start_slot 0
    pub fn init(&mut self, now: DateTime<Utc>) {
        let mut start_slot_time = now;
        for i in 0..self.slot_num {
            self.data[i] = Slot::new(start_slot_time);
            start_slot_time += Duration::milliseconds(self.interval);
        }
        self.start_slot = 0;
    }

    #[cfg(not(feature = "cache"))]
    // move_index range: 1--slot_num
    fn init_part(&mut self, move_index: i64) -> TardisResult<()> {
        if self.slot_num < move_index as usize {
            return Err(TardisError::bad_request("move index out of range", ""));
        }

        let last_slot_index = (self.start_slot + self.slot_num - 1) % self.slot_num;
        let mut move_slot_time = self.data[last_slot_index].time;
        move_slot_time += Duration::milliseconds(self.interval);

        for i in 0..move_index as usize {
            let index = (i + self.start_slot) % self.slot_num;
            self.data[index] = Slot::new(move_slot_time);
            move_slot_time += Duration::milliseconds(self.interval);
        }

        self.start_slot = (move_index as usize + self.start_slot) % self.slot_num;
        Ok(())
    }

    #[cfg(not(feature = "cache"))]
    pub fn add_one(&mut self, now: DateTime<Utc>) {
        let start_slot = &self.data[self.start_slot];
        let mut start_slot_time = start_slot.time;
        if start_slot_time + self.window_size <= now {
            if start_slot_time + self.window_size * 2 <= now {
                self.init(now);
            } else {
                let move_index = (now - start_slot_time - self.window_size).num_milliseconds() / self.interval + 1;
                self.init_part(move_index).expect("init part failed");
            }
            start_slot_time = self.data[self.start_slot].time;
        }
        // found a slot by now
        let slot_index = (now - start_slot_time).num_milliseconds() / self.interval;
        let add_slot_index = (slot_index as usize + self.start_slot) % self.slot_num;
        self.data[add_slot_index].count += 1;
    }

    #[cfg(not(feature = "cache"))]
    pub fn count_in_window(&self, now: DateTime<Utc>) -> u64 {
        self.data.iter().map(|slot| if (now - self.window_size) <= slot.time { slot.count } else { 0 }).sum::<u64>()
    }

    #[cfg(feature = "cache")]
    pub async fn add_and_count(&self, now: DateTime<Utc>, client: impl AsRef<tardis::cache::cache_client::TardisCacheClient>) -> TardisResult<u64> {
        let result: u64 = script()
            .key((if self.window_key.is_empty() { DEFAULT_CONF_WINDOW_KEY } else { &self.window_key }).to_string())
            .arg(self.window_size.num_milliseconds())
            .arg(now.timestamp())
            .arg(now.timestamp_subsec_micros())
            .invoke_async(&mut client.as_ref().cmd().await?)
            .await
            .map_err(|e| TardisError::internal_error(&format!("[SG.Filter.Status] redis error : {e}"), ""))?;
        Ok(result)
    }

    #[cfg(not(feature = "cache"))]
    pub fn add_and_count(&mut self, now: DateTime<Utc>) -> u64 {
        let result = self.count_in_window(now);
        self.add_one(now);
        result
    }

    #[cfg(not(feature = "cache"))]
    #[allow(dead_code)]
    fn get_data(&self) -> &[Slot] {
        &self.data
    }
}

#[cfg(not(feature = "cache"))]
#[derive(Default, Clone, Debug)]
struct Slot {
    time: DateTime<Utc>,
    count: u64,
}

#[cfg(not(feature = "cache"))]
impl Slot {
    fn new(start_time: DateTime<Utc>) -> Self {
        Slot { time: start_time, count: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "cache")]
    use crate::cache::Cache;
    #[cfg(feature = "cache")]
    use tardis::test::test_container::TardisTestContainer;
    #[cfg(feature = "cache")]
    use tardis::{testcontainers, tokio};

    #[test]
    #[cfg(not(feature = "cache"))]
    fn test() {
        let mut test = SlidingWindowCounter::new(Duration::seconds(60), 12);
        test.init(DateTime::parse_from_rfc3339("2000-01-01T01:00:00.000Z").unwrap().into());

        assert_eq!(test.get_data().len(), 12);
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:00:01.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:00:01.200Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:00:01.300Z").unwrap().into());
        assert_eq!(test.get_data()[0].count, 3);
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:00:59.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:00:59.200Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:00:59.300Z").unwrap().into());
        assert_eq!(test.get_data()[11].count, 3);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:01:00.000Z").unwrap().into()), 6);
        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:02:00.000Z").unwrap().into()), 0);

        // test add out of window time
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:00.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:00.200Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:00.300Z").unwrap().into());
        assert_eq!(test.get_data()[0].count, 3);
        assert_eq!(test.start_slot, 1);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:02:00.000Z").unwrap().into()), 3);

        //slide window
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:06.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:06.200Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:06.300Z").unwrap().into());
        assert_eq!(test.get_data()[1].count, 3);
        assert_eq!(test.start_slot, 2);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:02:00.000Z").unwrap().into()), 6);

        //slide window
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:50.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:01:50.200Z").unwrap().into());
        assert_eq!(test.get_data()[10].count, 2);
        assert_eq!(test.start_slot, 11);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:02:00.000Z").unwrap().into()), 8);

        //test reinit
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:03:05.100Z").unwrap().into());
        assert_eq!(test.get_data()[0].count, 1);
        assert_eq!(test.start_slot, 0);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:03:06.000Z").unwrap().into()), 1);

        //test critical case
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:04:05.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:04:05.100Z").unwrap().into());
        assert_eq!(test.get_data()[0].count, 2);
        assert_eq!(test.start_slot, 1);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:04:05.100Z").unwrap().into()), 2);

        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:05:10.100Z").unwrap().into());
        test.add_one(DateTime::parse_from_rfc3339("2000-01-01T01:05:10.100Z").unwrap().into());
        assert_eq!(test.get_data()[0].count, 2);
        assert_eq!(test.start_slot, 0);

        assert_eq!(test.count_in_window(DateTime::parse_from_rfc3339("2000-01-01T01:05:10.100Z").unwrap().into()), 2);
    }

    #[tokio::test]
    #[cfg(feature = "cache")]
    async fn test() {
        let _init = tardis::basic::tracing::TardisTracingInitializer::default().with_env_layer().with_fmt_layer().init();
        let docker = testcontainers::clients::Cli::default();
        let redis_container = TardisTestContainer::redis_custom(&docker);
        let port = redis_container.get_host_port_ipv4(6379);
        let url = format!("redis://127.0.0.1:{port}/0",);
        Cache::init("test_gate1", &url).await.unwrap();
        let client = Cache::get("test_gate1").await.unwrap();
        // fn new_ctx() -> SgRoutePluginContext {
        //     SgRoutePluginContext::new_http(
        //         Method::GET,
        //         Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
        //         Version::HTTP_11,
        //         HeaderMap::new(),
        //         Body::empty(),
        //         "127.0.0.1:8080".parse().unwrap(),
        //         "test_gate1".to_string(),
        //         None,
        //         None,
        //     )
        // }

        let test = SlidingWindowCounter::new(Duration::seconds(60), "");

        assert_eq!(
            test.add_and_count(DateTime::parse_from_rfc3339("2000-01-01T01:00:50.100Z").unwrap().into(), &client).await.unwrap(),
            0
        );
        assert_eq!(
            test.add_and_count(DateTime::parse_from_rfc3339("2000-01-01T01:00:55.100Z").unwrap().into(), &client).await.unwrap(),
            1
        );

        assert_eq!(
            test.add_and_count(DateTime::parse_from_rfc3339("2000-01-01T01:01:50.100Z").unwrap().into(), &client).await.unwrap(),
            1
        );
        assert_eq!(
            test.add_and_count(DateTime::parse_from_rfc3339("2000-01-01T01:01:55.000Z").unwrap().into(), &client).await.unwrap(),
            2
        );
        assert_eq!(
            test.add_and_count(DateTime::parse_from_rfc3339("2000-01-01T01:01:55.100Z").unwrap().into(), &client).await.unwrap(),
            2
        );

        assert_eq!(
            test.add_and_count(DateTime::parse_from_rfc3339("2000-01-01T01:05:00.100Z").unwrap().into(), &client).await.unwrap(),
            0
        );
    }
}
