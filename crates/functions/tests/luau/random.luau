local random = functions["random"]

test("is a whole number from the first to the second", function()
	for _ = 1, 200 do
		local n = tonumber(random("", "1 6"))
		eq(n >= 1 and n <= 6 and n == math.floor(n), true)
	end
	eq(random("", "-3 -3"), "-3")
	eq(random("", " 5  5 "), "5")
end)

test("is nothing without two numbers in order", function()
	eq(random("", nil), nil)
	eq(random("", "1"), nil)
	eq(random("", "6 1"), nil)
	eq(random("", "a b"), nil)
	eq(random("", "1.5 2"), nil)
end)

test("is nothing for a range past what it can draw from", function()
	eq(random("", "-2147483648 2147483647"), nil)
	eq(random("", "0 4294967296"), nil)
	eq(random("", "99999999999 99999999999"), nil)
end)

test("draws from a range as wide as it can", function()
	for _ = 1, 50 do
		local n = tonumber(random("", "0 2147483646"))
		eq(n >= 0 and n <= 2147483646, true)
	end
	eq(random("", "-2147483648 -2147483648"), "-2147483648")
end)
