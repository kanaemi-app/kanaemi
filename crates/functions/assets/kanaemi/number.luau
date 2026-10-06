-- kanaemi.number: reading numbers and writing them in kanji, for any function.
-- A number stays a list of its digits, so one past what Luau's numbers hold is
-- written whole.
local number = {}

local KANJI = { "〇", "一", "二", "三", "四", "五", "六", "七", "八", "九" }
local SMALL_UNITS = { "", "十", "百", "千" }
local LARGE_UNITS = { "", "万", "億", "兆", "京" }

-- The value of each digit of `source`, ASCII or full-width, first to last;
-- nil when it has anything else, or nothing.
function number.digits(source)
	local found = {}
	for _, code in utf8.codes(source) do
		if code >= 0x30 and code <= 0x39 then
			table.insert(found, code - 0x30)
		elseif code >= 0xFF10 and code <= 0xFF19 then
			table.insert(found, code - 0xFF10)
		else
			return nil
		end
	end
	if #found == 0 then
		return nil
	end
	return found
end

-- The number in kanji counted by its units, without its leading zeros, or nil
-- past the largest unit. `daiji` writes every one, and 壱・弐・参・拾 for
-- 一・二・三・十.
function number.counted(digits, daiji)
	local first = 1
	while first < #digits and digits[first] == 0 do
		first += 1
	end
	digits = table.move(digits, first, #digits, 1, {})
	if #digits > #SMALL_UNITS * #LARGE_UNITS then
		return nil
	end
	if #digits == 1 and digits[1] == 0 then
		return KANJI[1]
	end
	-- Groups of four from the ones, so each lines up with its unit.
	local groups = {}
	local last = #digits
	while last >= 1 do
		table.insert(groups, table.move(digits, math.max(1, last - 3), last, 1, {}))
		last -= 4
	end
	local out = {}
	for at = #groups, 1, -1 do
		local group = groups[at]
		local written = false
		for place, digit in ipairs(group) do
			local small = SMALL_UNITS[#group - place + 1]
			if digit ~= 0 then
				if digit ~= 1 or small == "" or daiji then
					table.insert(out, KANJI[digit + 1])
				end
				table.insert(out, small)
				written = true
			end
		end
		if written then
			table.insert(out, LARGE_UNITS[at])
		end
	end
	local text = table.concat(out)
	if daiji then
		text = text:gsub("一", "壱"):gsub("二", "弐"):gsub("三", "参"):gsub("十", "拾")
	end
	return text
end

return number
