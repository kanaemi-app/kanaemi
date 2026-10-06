-- Dates as the built-in functions that write one count and write them: a table
-- of year, month, day, hour, min and sec, and offset, the seconds its time is
-- ahead of UTC. Worked out without the clock's time zone rules, so a date moves
-- by whole days.
local date = {}

-- Each era from its first day, the latest first.
local ERAS = {
	{ "令和", 2019, 5, 1 },
	{ "平成", 1989, 1, 8 },
	{ "昭和", 1926, 12, 25 },
	{ "大正", 1912, 7, 30 },
	{ "明治", 1868, 10, 23 },
}
local WEEKDAYS = { "日", "月", "火", "水", "木", "金", "土" }

-- Days from 1970-01-01 to the date.
function date.days(year, month, day)
	local y = month <= 2 and year - 1 or year
	local era = y // 400
	local of_era = y - era * 400
	local of_year = (153 * (month > 2 and month - 3 or month + 9) + 2) // 5 + day - 1
	local of_era_days = of_era * 365 + of_era // 4 - of_era // 100 + of_year
	return era * 146097 + of_era_days - 719468
end

-- The year, month and day `days` from 1970-01-01.
function date.civil(days)
	local z = days + 719468
	local era = z // 146097
	local of_era = z - era * 146097
	local year_of_era = (of_era - of_era // 1460 + of_era // 36524 - of_era // 146096) // 365
	local of_year = of_era - (365 * year_of_era + year_of_era // 4 - year_of_era // 100)
	local mp = (5 * of_year + 2) // 153
	local day = of_year - (153 * mp + 2) // 5 + 1
	local month = mp < 10 and mp + 3 or mp - 9
	local year = year_of_era + era * 400 + (month <= 2 and 1 or 0)
	return year, month, day
end

-- `t` moved by `shift`: "+N" or "-N" days, or with "y" after it years, a 29
-- February then falling on the last day of February. nil for another shift.
function date.moved(t, shift)
	local sign, count, unit = shift:match("^([+-])(%d+)(y?)$")
	if not sign then
		return nil
	end
	local by = tonumber(count) * (sign == "-" and -1 or 1)
	local moved = table.clone(t)
	if unit == "y" then
		moved.year += by
		if moved.month == 2 and moved.day == 29 then
			local _, _, last_day = date.civil(date.days(moved.year, 3, 1) - 1)
			moved.day = last_day
		end
	else
		moved.year, moved.month, moved.day = date.civil(date.days(t.year, t.month, t.day) + by)
	end
	return moved
end

-- The year of an era its first year is the western year of; nil for an era
-- there is none of, and for a year past the era.
function date.western(era_name, year)
	for at, era in ERAS do
		if era[1] == era_name then
			local next_era = ERAS[at - 1]
			local last = next_era and next_era[2] - era[2] + 1 or math.huge
			return year >= 1 and year <= last and era[2] + year - 1 or nil
		end
	end
	return nil
end

-- The era of `t` and the year of it; nil before 明治.
function date.era(t)
	local key = t.year * 10000 + t.month * 100 + t.day
	for _, era in ERAS do
		if key >= era[2] * 10000 + era[3] * 100 + era[4] then
			return era[1], t.year - era[2] + 1
		end
	end
	return nil
end

-- `t` written as `format` says: %Y year; %m and %d month and day, %H, %M and
-- %S hour, minute and second, all of two digits; %-m, %-d, %-H and %-M the
-- same without a leading zero; %-I the hour on a twelve-hour clock and %p 午前
-- or 午後; %a the day of the week; %E the era and %N its year; %z and %:z the
-- offset from UTC (+0900, +09:00); %s the seconds since 1970-01-01 UTC; %% a
-- %. Other text is written as it is. nil when an era is asked for a date
-- before 明治.
function date.format(t, format)
	local era, era_year = date.era(t)
	local missing = false
	local function two(value, padded)
		return padded and string.format("%02d", value) or tostring(value)
	end
	local text = format:gsub("%%([%-:]?)(.)", function(flag, key)
		if flag == ":" then
			return key == "z" and date.offset(t.offset, ":") or nil
		end
		local padded = flag == ""
		if key == "m" then
			return two(t.month, padded)
		elseif key == "d" then
			return two(t.day, padded)
		elseif key == "H" then
			return two(t.hour, padded)
		elseif key == "M" then
			return two(t.min, padded)
		elseif not padded and key == "I" then
			return tostring(t.hour % 12)
		elseif not padded then
			return nil
		elseif key == "Y" then
			return tostring(t.year)
		elseif key == "S" then
			return two(t.sec, true)
		elseif key == "p" then
			return t.hour < 12 and "午前" or "午後"
		elseif key == "a" then
			local weekday = (date.days(t.year, t.month, t.day) + 4) % 7
			return WEEKDAYS[weekday + 1]
		elseif key == "E" or key == "N" then
			missing = missing or era == nil
			return era and (key == "E" and era or tostring(era_year)) or ""
		elseif key == "z" then
			return date.offset(t.offset, "")
		elseif key == "s" then
			local seconds = date.days(t.year, t.month, t.day) * 86400 + t.hour * 3600 + t.min * 60 + t.sec
			return tostring(seconds - t.offset)
		elseif key == "%" then
			return "%"
		end
		return nil
	end)
	return not missing and text or nil
end

-- `offset` seconds ahead of UTC as hours and minutes, `between` them.
function date.offset(offset, between)
	local sign = offset < 0 and "-" or "+"
	local minutes = math.abs(offset) // 60
	return string.format("%s%02d%s%02d", sign, minutes // 60, between, minutes % 60)
end

-- Seconds from 1970-01-01 of `t` as if it were UTC.
local function seconds(t)
	return date.days(t.year, t.month, t.day) * 86400 + t.hour * 3600 + t.min * 60 + t.sec
end

-- The seconds the clock's time zone is ahead of UTC at `instant`; nil when
-- the clock cannot tell, as before 1970.
local function offset_at(instant)
	local here, utc = os.date("*t", instant), os.date("!*t", instant)
	return here and utc and seconds(here) - seconds(utc)
end

-- `t` with the offset the clock's time zone has at its date and time, which
-- may differ from its own across a change of summer time; its own where the
-- clock cannot tell.
function date.zoned(t)
	local zoned = table.clone(t)
	-- Asked again with the offset found, as one that changed at the date may
	-- name another instant.
	for _ = 1, 2 do
		zoned.offset = offset_at(seconds(t) - zoned.offset) or zoned.offset
	end
	return zoned
end

-- Now, in the time zone of the clock.
function date.now()
	local now = os.time()
	local here = os.date("*t", now)
	return {
		year = here.year,
		month = here.month,
		day = here.day,
		hour = here.hour,
		min = here.min,
		sec = here.sec,
		offset = offset_at(now),
	}
end

return date
