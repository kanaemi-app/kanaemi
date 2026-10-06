local number = kanaemi.number

test("digits reads ASCII and full-width digits, mixed", function()
	local digits = number.digits("1２0")
	eq(#digits, 3)
	eq(digits[1], 1)
	eq(digits[2], 2)
	eq(digits[3], 0)
end)

test("digits gives nil for anything but digits, or nothing", function()
	eq(number.digits("1こ"), nil)
	eq(number.digits("-1"), nil)
	eq(number.digits("1.5"), nil)
	eq(number.digits(""), nil)
end)

test("counted counts in groups of ten thousand", function()
	local cases = {
		{ "10000", "一万" },
		{ "1000000", "百万" },
		{ "100010", "十万十" },
		{ "1111", "千百十一" },
		{ "10000000", "千万" },
		{ "100000000", "一億" },
		{ "1000000000000", "一兆" },
		{ "10000000000000000", "一京" },
		{
			"99999999999999999999",
			"九千九百九十九京九千九百九十九兆九千九百九十九億九千九百九十九万九千九百九十九",
		},
	}
	for _, case in cases do
		eq(number.counted(number.digits(case[1]), false), case[2])
	end
end)

test("counted leaves out the leading zeros and writes zero as 〇", function()
	eq(number.counted(number.digits("007"), false), "七")
	eq(number.counted(number.digits("000"), false), "〇")
end)

test("counted has nothing for ten thousand kei and above", function()
	eq(number.counted(number.digits("100000000000000000000"), false), nil)
	eq(number.counted(number.digits("100000000000000000000"), true), nil)
end)

test("counted as daiji writes every one and changes only its own characters", function()
	eq(number.counted(number.digits("1111"), true), "壱千壱百壱拾壱")
	eq(number.counted(number.digits("10"), true), "壱拾")
	eq(number.counted(number.digits("10000"), true), "壱万")
	eq(number.counted(number.digits("345"), true), "参百四拾五")
end)
