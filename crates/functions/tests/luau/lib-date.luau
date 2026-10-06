local date = require("./lib/date")

local function at(year, month, day, hour, min, sec)
	return { year = year, month = month, day = day, hour = hour or 0, min = min or 0, sec = sec or 0 }
end

test("days and civil count from 1970-01-01 and back", function()
	eq(date.days(1970, 1, 1), 0)
	eq(date.days(2026, 10, 7), 20733)
	for _, days in { -1, 0, 59, 11016, 20733, 40000 } do
		local year, month, day = date.civil(days)
		eq(date.days(year, month, day), days)
	end
end)

test("writes the parts of a date and a time", function()
	local t = at(2026, 10, 7, 13, 5, 9)
	eq(date.format(t, "%Y-%m-%d %H:%M:%S"), "2026-10-07 13:05:09")
	eq(date.format(at(2026, 3, 4, 9, 5), "%-m月%-d日%-H時%-M分"), "3月4日9時5分")
end)

test("writes the hour on a twelve-hour clock with 午前 or 午後", function()
	eq(date.format(at(2026, 10, 7, 13, 5), "%p%-I時"), "午後1時")
	eq(date.format(at(2026, 10, 7, 0, 5), "%p%-I時"), "午前0時")
	eq(date.format(at(2026, 10, 7, 12, 0), "%p%-I時"), "午後0時")
end)

test("writes the day of the week", function()
	eq(date.format(at(2026, 10, 7), "%a"), "水")
	eq(date.format(at(1970, 1, 1), "%a"), "木")
	eq(date.format(at(2026, 10, 11), "%a"), "日")
end)

test("writes the era from its first day", function()
	eq(date.format(at(2026, 10, 7), "%E%N年"), "令和8年")
	eq(date.format(at(2019, 5, 1), "%E%N年"), "令和1年")
	eq(date.format(at(2019, 4, 30), "%E%N年"), "平成31年")
	eq(date.format(at(1989, 1, 7), "%E%N年"), "昭和64年")
	eq(date.format(at(1868, 10, 23), "%E%N年"), "明治1年")
end)

test("has nothing of an era before 明治", function()
	eq(date.format(at(1868, 10, 22), "%E%N年"), nil)
	eq(date.format(at(1868, 10, 22), "%Y年"), "1868年")
end)

test("writes a percent sign and leaves other text as it is", function()
	eq(date.format(at(2026, 10, 7), "%% %Q x"), "% %Q x")
end)

test("moves by days across months and years", function()
	local moved = date.moved(at(2026, 12, 31), "+1")
	eq(date.format(moved, "%Y-%m-%d"), "2027-01-01")
	eq(date.format(date.moved(at(2024, 3, 1), "-1"), "%Y-%m-%d"), "2024-02-29")
	eq(date.format(date.moved(at(2026, 10, 7), "+0"), "%Y-%m-%d"), "2026-10-07")
end)

test("moves by years, 29 February falling on the last day of February", function()
	eq(date.format(date.moved(at(2026, 10, 7), "+1y"), "%Y-%m-%d"), "2027-10-07")
	eq(date.format(date.moved(at(2024, 2, 29), "+1y"), "%Y-%m-%d"), "2025-02-28")
	eq(date.format(date.moved(at(2024, 2, 29), "-4y"), "%Y-%m-%d"), "2020-02-29")
end)

test("moves by nothing else", function()
	eq(date.moved(at(2026, 10, 7), "1"), nil)
	eq(date.moved(at(2026, 10, 7), "+1m"), nil)
end)

test("writes the offset from UTC", function()
	local t = at(2026, 10, 7, 13, 5, 9)
	t.offset = 9 * 3600
	eq(date.format(t, "%z %:z"), "+0900 +09:00")
	t.offset = -(5 * 3600 + 30 * 60)
	eq(date.format(t, "%z %:z"), "-0530 -05:30")
end)

test("writes the seconds since 1970-01-01 UTC", function()
	local t = at(1970, 1, 1, 9, 0, 0)
	t.offset = 9 * 3600
	eq(date.format(t, "%s"), "0")
	local later = at(2026, 10, 7, 13, 5, 9)
	later.offset = 9 * 3600
	eq(date.format(later, "%s"), "1791345909")
end)

test("now has its offset from UTC", function()
	local now = date.now()
	eq(type(now.offset), "number")
	eq(now.offset % 60, 0)
end)

test("western gives the western year of a year of an era", function()
	eq(date.western("令和", 8), 2026)
	eq(date.western("令和", 1), 2019)
	eq(date.western("平成", 31), 2019)
	eq(date.western("昭和", 64), 1989)
	eq(date.western("明治", 45), 1912)
end)

test("western has nothing past the era or of another name", function()
	eq(date.western("平成", 32), nil)
	eq(date.western("令和", 0), nil)
	eq(date.western("天平", 1), nil)
end)

test("zoned keeps the offset of now for now, and gives one for another day", function()
	local now = date.now()
	eq(date.zoned(now).offset, now.offset)
	local later = date.zoned(date.moved(now, "+180"))
	eq(type(later.offset), "number")
	eq(later.offset % 60, 0)
end)

test("a zoned date names the instant the clock shows as its date and time", function()
	local now = date.now()
	for _, month in { 1, 4, 7, 10 } do
		local t = at(now.year, month, 15, 12, 0, 0)
		t.offset = now.offset
		t = date.zoned(t)
		local shown = os.date("*t", tonumber(date.format(t, "%s")))
		eq(string.format("%d-%d %d:%d", shown.month, shown.day, shown.hour, shown.min), `{month}-15 12:0`)
	end
end)

test("zoned keeps the offset for a date the clock cannot tell", function()
	local t = at(1960, 6, 1, 12, 0, 0)
	t.offset = 9 * 3600
	eq(date.zoned(t).offset, 9 * 3600)
end)

test("days and civil go back and forth for years before the common era", function()
	for _, date_of in { { -4, 3, 1 }, { 0, 1, 1 }, { -1, 12, 31 }, { -400, 2, 29 } } do
		local year, month, day = date.civil(date.days(date_of[1], date_of[2], date_of[3]))
		eq(`{year}-{month}-{day}`, `{date_of[1]}-{date_of[2]}-{date_of[3]}`)
	end
end)
