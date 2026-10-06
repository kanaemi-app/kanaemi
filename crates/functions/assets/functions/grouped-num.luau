-- grouped-num: a number in ASCII digits with a comma between each group of
-- three, without its leading zeros (2026 → 2,026).
local number = require("./lib/number")

return function(source)
	local digits = kanaemi.number.digits(source)
	if not digits then
		return nil
	end
	digits = number.significant(digits)
	local out = {}
	for at, digit in ipairs(digits) do
		if at > 1 and (#digits - at + 1) % 3 == 0 then
			table.insert(out, ",")
		end
		table.insert(out, number.ASCII[digit + 1])
	end
	return table.concat(out)
end
