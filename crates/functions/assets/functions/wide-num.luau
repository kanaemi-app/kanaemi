-- wide-num: a number in full-width digits, as typed otherwise (007 → ００７).
local number = require("./lib/number")

return function(source)
	local digits = kanaemi.number.digits(source)
	return digits and number.each(digits, number.WIDE)
end
