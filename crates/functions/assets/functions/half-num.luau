-- half-num: a number in ASCII digits, as typed otherwise (００７ → 007).
local number = require("./lib/number")

return function(source)
	local digits = kanaemi.number.digits(source)
	return digits and number.each(digits, number.ASCII)
end
