-- 引数の元号の年を西暦で書く（令和の 8 → 2026）
-- seireki: a year of the era the argument names, as a western year ("8" in
-- 令和 → "2026"). nil for a year past the era.
local date = require("./lib/date")

return function(source, argument)
	local digits = kanaemi.number.digits(source)
	if not digits or #digits > 4 or not argument then
		return nil
	end
	local year = date.western(argument, tonumber(table.concat(digits)))
	return year and tostring(year)
end
