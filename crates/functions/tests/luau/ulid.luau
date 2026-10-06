local ulid = functions["ulid"]
local CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

test("is 26 letters of Crockford's base 32", function()
	local id = ulid("")
	eq(#id, 26)
	eq(id:match("^[0-9A-HJKMNP-TV-Z]+$") ~= nil, true)
end)

test("is another each time", function()
	eq(ulid("") ~= ulid(""), true)
end)

test("has the time of now to the millisecond", function()
	local before = kanaemi.time.milliseconds()
	local id = ulid("")
	local after = kanaemi.time.milliseconds()
	local time = 0
	for at = 1, 10 do
		time = time * 32 + (CROCKFORD:find(id:sub(at, at), 1, true) - 1)
	end
	eq(time >= before and time <= after, true)
end)
