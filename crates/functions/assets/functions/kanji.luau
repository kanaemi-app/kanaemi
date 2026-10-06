-- kanji: a number in kanji counted by its units (2026 → 二千二十六). A one before
-- 十・百・千 is left out; past the largest unit, 京, there is none.
return function(source)
	local digits = kanaemi.number.digits(source)
	return digits and kanaemi.number.counted(digits, false)
end
