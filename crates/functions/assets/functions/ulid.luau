-- ULID を作る
-- ulid: a ULID of now. Its random bits come from kanaemi.random, as one must
-- never repeat.
local CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

local function letter(value)
	return CROCKFORD:sub(value + 1, value + 1)
end

return function()
	local out = {}
	local time = kanaemi.time.milliseconds()
	for at = 10, 1, -1 do
		out[at] = letter(time % 32)
		time //= 32
	end
	-- 80 random bits, five at a time.
	local bits, held = 0, 0
	for _, byte in { string.byte(kanaemi.random.bytes(10), 1, 10) } do
		bits = bits * 256 + byte
		held += 8
		while held >= 5 do
			held -= 5
			local value = bits // 2 ^ held
			table.insert(out, letter(value))
			bits -= value * 2 ^ held
		end
	end
	return table.concat(out)
end
