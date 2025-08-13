-- Use `counter` to accumulate 1 for each request
local current_count = tonumber(redis.call('incr', KEYS[1]));
if current_count == 1 then
    -- The current request is the first request, record the current timestamp
    redis.call('set', KEYS[2], ARGV[3]);
end
-- When the `counter` value reaches the maximum number of requests
if current_count > tonumber(ARGV[1]) then
    local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
    if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
        -- Last reset time + time window > current time,
        -- indicating that the request has reached the upper limit within this time period,
        -- so the request is limited
        return 0;
    end
    -- Otherwise reset the counter and timestamp,
    -- and allow the request
    redis.call('set', KEYS[1], '1')
    redis.call('set', KEYS[2], ARGV[3]);
end
return 1;