-- 数を位取りした大字で書く（2026 → 弐千弐拾六）
-- daiji: a number in daiji counted by its units (2026 → 弐千弐拾六). Every one is
-- written; past the largest unit, 京, there is none.
return function(source)
	local digits = kanaemi.number.digits(source)
	return digits and kanaemi.number.counted(digits, true)
end
