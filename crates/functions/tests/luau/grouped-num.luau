local grouped = functions["grouped-num"]

test("puts a comma between each group of three", function()
	eq(grouped("12"), "12")
	eq(grouped("2026"), "2,026")
	eq(grouped("1234567"), "1,234,567")
	eq(grouped("２０２６"), "2,026")
end)

test("leaves out the leading zeros", function()
	eq(grouped("007"), "7")
	eq(grouped("000"), "0")
end)

test("writes a number past what Luau's numbers hold whole", function()
	eq(grouped("100000000000000000000"), "100,000,000,000,000,000,000")
end)

test("gives nil for what is no number", function()
	eq(grouped("きょう"), nil)
end)
