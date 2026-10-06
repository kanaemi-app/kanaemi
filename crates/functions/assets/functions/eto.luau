-- 今年の干支を書く（丙午）。引数を 支 にすると十二支だけ（午）
-- eto: the sexagenary sign of this year (2026 → 丙午); with the argument 支,
-- only its zodiac animal (午).
local date = require("./lib/date")

local STEMS = { "甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸" }
local BRANCHES = { "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥" }

return function(source, argument)
	local year = date.now().year
	local branch = BRANCHES[(year - 4) % 12 + 1]
	if argument == "支" then
		return branch
	end
	return STEMS[(year - 4) % 10 + 1] .. branch
end
