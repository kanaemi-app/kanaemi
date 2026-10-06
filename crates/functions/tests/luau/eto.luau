local eto = functions["eto"]
local STEMS = "甲乙丙丁戊己庚辛壬癸"
local BRANCHES = "子丑寅卯辰巳午未申酉戌亥"

test("is the sign of this year", function()
	local sign = eto("")
	eq(utf8.len(sign), 2)
	local stem = sign:sub(1, utf8.offset(sign, 2) - 1)
	local branch = sign:sub(utf8.offset(sign, 2))
	eq(STEMS:find(stem, 1, true) ~= nil, true)
	eq(BRANCHES:find(branch, 1, true) ~= nil, true)
	eq(eto("", "支"), branch)
end)

test("goes round sixty years from 甲子 in 1984", function()
	local year = tonumber(os.date("%Y"))
	local stems = { "甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸" }
	local branches = { "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥" }
	eq(eto(""), stems[(year - 1984) % 10 + 1] .. branches[(year - 1984) % 12 + 1])
end)
