-- 引数の 2 つの数のあいだの整数を 1 つ選ぶ（1 6 → 4）
-- random: a whole number from the first to the second in the argument
-- ("1 100"), in ASCII digits. nil without two such numbers in order, or for
-- a range math.random cannot draw from: numbers of 32 bits, less than 2^31 - 1
-- apart.
return function(source, argument)
	local low, high = (argument or ""):match("^%s*(%-?%d+)%s+(%-?%d+)%s*$")
	low, high = tonumber(low), tonumber(high)
	if not low or not high or low > high or low < -2 ^ 31 or high >= 2 ^ 31 or high - low >= 2 ^ 31 - 1 then
		return nil
	end
	return tostring(math.random(low, high))
end
