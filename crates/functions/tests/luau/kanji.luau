local kanji = functions["kanji"]

test("counts by its units", function()
	eq(kanji("12"), "十二")
	eq(kanji("2026"), "二千二十六")
	eq(kanji("２０２６"), "二千二十六")
	eq(kanji("1111"), "千百十一")
	eq(kanji("10000"), "一万")
end)

test("leaves out the leading zeros and writes zero as 〇", function()
	eq(kanji("007"), "七")
	eq(kanji("0"), "〇")
	eq(kanji("000"), "〇")
end)

test("has nothing for ten thousand kei and above", function()
	eq(kanji("100000000000000000000"), nil)
end)

test("gives nil for what is no number", function()
	eq(kanji("きょう"), nil)
	eq(kanji(""), nil)
end)
