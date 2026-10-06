local seireki = functions["seireki"]

test("writes a year of the era as a western year", function()
	eq(seireki("8", "令和"), "2026")
	eq(seireki("８", "令和"), "2026")
	eq(seireki("31", "平成"), "2019")
	eq(seireki("64", "昭和"), "1989")
end)

test("has nothing past the era, of another era, or without one", function()
	eq(seireki("32", "平成"), nil)
	eq(seireki("1", "天平"), nil)
	eq(seireki("8", nil), nil)
	eq(seireki("きょう", "令和"), nil)
end)
