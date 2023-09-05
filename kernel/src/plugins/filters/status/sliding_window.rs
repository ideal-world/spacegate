use lazy_static::lazy_static;
use tardis::basic::{error::TardisError, result::TardisResult};
#[cfg(feature = "cache")]
use tardis::cache::Script;
use tardis::chrono::{DateTime, Duration, Utc};

#[cfg(feature = "cache")]
const CONF_WINDOW_KEY: &str = "sg:plugin:filter:window:key";

#[cfg(feature = "cache")]
lazy_static! {
    /// Sliding window script
    ///
    /// # Arguments
    ///
    /// * KEYS[1]  window key
    /// * ARGV[1]  interval
    /// * ARGV[2]  window size
    /// * ARGV[3]  current timestamp
    ///
    /// # Return
    ///
    /// * count
    ///
    /// # Kernel logic
    ///
    static ref SCRIPT: Script = Script::new(
        r"
    local key = KEYS[1]
    local microsecFactor = tonumber(KEYS[2])

    local window_size = tonumber(ARGV[1])
    local current_time = tonumber(ARGV[2])
    local interval = tonumber(ARGV[1])

    local member_score = current_time  -- Compute member score (timestamp) at the right subdivision unit using the conversion factor
    local window_expire_at = current_time + window_size -- Window expiration epoch in milliseconds

    -- Remove elements older than (now - window) at the given subdivision unit
    redis.call('ZREMRANGEBYSCORE', key, 0, (current_time - window_size))
    
    -- Get number of requests in the current window
    local current_requests_count = redis.call('ZCARD', key)

    redis.call('ZADD', key, member_score, now_microsec)

    redis.call('PEXPIRE', key, window_expire_at)

    return current_requests_count
    ",
    );
}

pub(crate) struct SlidingWindowCounter {
    window_size: Duration,
    data: Vec<Slot>,
    // slot_num equal to data.len()
    slot_num: usize,
    // milliseconds
    interval: i64,
    // range: 0--(slot_num-1)
    start_slot: usize,
}

impl SlidingWindowCounter {
    pub(crate) fn new(window_size: Duration, slot_num: usize) -> Self {
        let interval = window_size.num_milliseconds() / slot_num as i64;
        let mut result = SlidingWindowCounter {
            window_size,
            data: vec![Slot::default(); slot_num],
            slot_num,
            interval,
            start_slot: 0,
        };
        result.init(Utc::now());
        result
    }

    /// Initialize the sliding window ,set start_slot 0
    fn init(&mut self, now: DateTime<Utc>) {
        let mut start_slot_time = now;
        for i in 0..self.slot_num {
            self.data[i] = Slot::new(start_slot_time);
            start_slot_time = start_slot_time + Duration::milliseconds(self.interval);
        }
        self.start_slot = 0;
    }

    // move_idex range: 1--slot_num
    fn init_part(&mut self, move_index: i64) -> TardisResult<()> {
        if self.slot_num < move_index as usize {
            return Err(TardisError::bad_request("move index out of range", ""));
        }

        let last_slot_index = (self.start_slot + self.slot_num - 1) % self.slot_num;
        let mut move_slot_time = self.data[last_slot_index].time.clone();
        move_slot_time = move_slot_time + Duration::milliseconds(self.interval);

        for i in 0..move_index as usize {
            let index = (i + self.start_slot) % self.slot_num;
            self.data[index] = Slot::new(move_slot_time);
            move_slot_time = move_slot_time + Duration::milliseconds(self.interval);
        }

        self.start_slot = (move_index as usize + self.start_slot) % self.slot_num;
        Ok(())
    }

    pub(crate) fn add_one(&mut self, now: DateTime<Utc>) {
        let start_slot = &self.data[self.start_slot];
        let mut start_slot_time = start_slot.time.clone();
        if start_slot_time + self.window_size <= now {
            if start_slot_time + self.window_size * 2 <= now {
                self.init(now);
            } else {
                let move_index = (now - start_slot_time - self.window_size).num_milliseconds() / self.interval as i64 + 1;
                self.init_part(move_index).expect("init part failed");
            }
            start_slot_time = self.data[self.start_slot].time.clone();
        }
        // found a slot by now
        let slot_index = (now - start_slot_time).num_milliseconds() / self.interval as i64;
        let add_slot_index = (slot_index as usize + self.start_slot) % self.slot_num;
        self.data[add_slot_index].count += 1;
    }

    pub(crate) fn count_in_window(&self, now: DateTime<Utc>) -> u64 {
        self.data.iter().map(|slot| if (now - self.window_size) <= slot.time { slot.count } else { 0 }).sum::<u64>()
    }

    pub(crate) fn add_and_count(&mut self, now: DateTime<Utc>) -> u64 {
        let result =self.count_in_window(now);
        self.add_one(now);
        result
    }

    fn get_data(&self) -> &[Slot] {
        &self.data
    }
}

#[derive(Default, Clone, Debug)]
struct Slot {
    time: DateTime<Utc>,
    count: u64,
}

impl Slot {
    fn new(start_time: DateTime<Utc>) -> Self {
        Slot { time: start_time, count: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut test = SlidingWindowCounter::new(Duration::seconds(60), 12);
        test.init(DateTime::parse_from_rfc3339("2000-01-01T01:00:00.000Z").unwrap().into());

        assert!(test.get_data().len() == 12);
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
}
