-- 西暦の年を和暦で書く（2026 → 令和8）
-- wareki: a western year in the era of its last day ("2026" → "令和8"), or of
-- the day the argument names as month-day ("1-1"), for a year an era began in.
local date = require("./lib/date")

return function(source, argument)
	local digits = kanaemi.number.digits(source)
	if not digits or #digits > 4 then
		return nil
	end
	local year = tonumber(table.concat(digits))
	local month, day = 12, 31
	if argument then
		local m, d = argument:match("^(%d+)%-(%d+)$")
		month, day = tonumber(m), tonumber(d)
		if not month or month < 1 or month > 12 or day < 1 then
			return nil
		end
		-- A day the month does not have, such as 2-31, is no day of the year.
		local _, real_month, real_day = date.civil(date.days(year, month, day))
		if real_month ~= month or real_day ~= day then
			return nil
		end
	end
	local era, era_year = date.era({ year = year, month = month, day = day })
	return era and era .. era_year
end
