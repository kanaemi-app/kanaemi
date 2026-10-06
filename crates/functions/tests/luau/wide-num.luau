local wide = functions["wide-num"]

test("writes full-width digits", function()
	eq(wide("12"), "１２")
	eq(wide("2026"), "２０２６")
	eq(wide("1２"), "１２")
end)

test("keeps the leading zeros", function()
	eq(wide("007"), "００７")
end)

test("gives nil for what is no number", function()
	eq(wide("きょう"), nil)
	eq(wide(""), nil)
end)
