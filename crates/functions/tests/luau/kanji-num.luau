local kanji_num = functions["kanji-num"]

test("writes kanji digit by digit with 0 as 〇", function()
	eq(kanji_num("12"), "一二")
	eq(kanji_num("2026"), "二〇二六")
	eq(kanji_num("２０２６"), "二〇二六")
end)

test("keeps the leading zeros", function()
	eq(kanji_num("007"), "〇〇七")
end)

test("gives nil for what is no number", function()
	eq(kanji_num("きょう"), nil)
	eq(kanji_num(""), nil)
end)
