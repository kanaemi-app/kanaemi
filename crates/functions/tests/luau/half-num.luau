local half = functions["half-num"]

test("writes ASCII digits", function()
	eq(half("12"), "12")
	eq(half("２０２６"), "2026")
	eq(half("1２"), "12")
end)

test("keeps the leading zeros", function()
	eq(half("007"), "007")
end)

test("writes a number past what Luau's numbers hold whole", function()
	eq(half("100000000000000000000"), "100000000000000000000")
end)

test("gives nil for what is no number", function()
	eq(half("きょう"), nil)
	eq(half(""), nil)
end)
