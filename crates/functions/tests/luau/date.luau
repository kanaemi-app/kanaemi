local date = functions["date"]

test("writes today as %Y-%m-%d without an argument", function()
	eq(date("") ~= nil and date(""):match("^%d%d%d%d%-%d%d%-%d%d$") ~= nil, true)
	eq(date("", nil), os.date("%Y-%m-%d"))
end)

test("writes now as the argument says", function()
	eq(date("", "%Y"), os.date("%Y"))
end)

test("moves the date by the shift before the format", function()
	local tomorrow = date("", "+1 %Y-%m-%d")
	local yesterday = date("", "-1 %Y-%m-%d")
	eq(tomorrow ~= date("", "%Y-%m-%d"), true)
	eq(yesterday < date("", "%Y-%m-%d"), true)
	eq(date("", "+1y %Y"), tostring(tonumber(os.date("%Y")) + 1))
	eq(date("", "+1"):match("^%d%d%d%d%-%d%d%-%d%d$") ~= nil, true)
end)

test("moves to a year before 1970", function()
	eq(date("", "-60y %Y"), tostring(tonumber(os.date("%Y")) - 60))
end)
