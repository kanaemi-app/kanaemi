-- What the built-in functions share besides kanaemi.number. Copied into the
-- functions folder with them, it is required the same way there.
local number = {}

number.ASCII = { "0", "1", "2", "3", "4", "5", "6", "7", "8", "9" }
number.WIDE = { "０", "１", "２", "３", "４", "５", "６", "７", "８", "９" }
number.KANJI = { "〇", "一", "二", "三", "四", "五", "六", "七", "八", "九" }

-- `digits` without its leading zeros; zero itself stays one zero.
function number.significant(digits)
	local first = 1
	while first < #digits and digits[first] == 0 do
		first += 1
	end
	return table.move(digits, first, #digits, 1, {})
end

-- Each digit written as `written` has it, from 0.
function number.each(digits, written)
	local out = {}
	for _, digit in ipairs(digits) do
		table.insert(out, written[digit + 1])
	end
	return table.concat(out)
end

return number
