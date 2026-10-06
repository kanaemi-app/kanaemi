-- kanji-num: a number in kanji digit by digit, 0 as 〇 (2026 → 二〇二六).
local number = require("./lib/number")

return function(source)
	local digits = kanaemi.number.digits(source)
	return digits and number.each(digits, number.KANJI)
end
