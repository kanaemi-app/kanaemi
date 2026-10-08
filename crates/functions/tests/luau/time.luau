local milliseconds = kanaemi.time.milliseconds

test("tells now in milliseconds since 1970-01-01 UTC", function()
	local before = os.time()
	local now = milliseconds()
	local after = os.time()
	-- `os.time` may read a coarser clock, which on Linux lags behind by up to
	-- a tick, so `after` can still be the second before `now`'s.
	eq(now >= (before - 1) * 1000 and now < (after + 2) * 1000, true)
	eq(now == math.floor(now), true)
end)

test("goes on, never back", function()
	local first = milliseconds()
	eq(milliseconds() >= first, true)
end)
