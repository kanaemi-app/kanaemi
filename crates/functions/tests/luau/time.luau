local milliseconds = kanaemi.time.milliseconds

test("tells now in milliseconds since 1970-01-01 UTC", function()
	local before = os.time() * 1000
	local now = milliseconds()
	eq(now >= before and now < (os.time() + 1) * 1000, true)
	eq(now == math.floor(now), true)
end)

test("goes on, never back", function()
	local first = milliseconds()
	eq(milliseconds() >= first, true)
end)
