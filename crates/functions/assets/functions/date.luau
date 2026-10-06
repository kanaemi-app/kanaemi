-- いまの日時を、引数の書式で書く（%Y-%m-%d → 2026-10-07、+1 で明日）
-- date: now, written as the argument says ("%Y-%m-%d" without one). A shift
-- before it moves the date: "+1 %Y-%m-%d" tomorrow, "-1y %Y年" last year.
-- See lib/date.luau for what the argument may write.
local date = require("./lib/date")

return function(source, argument)
	local t = date.now()
	local format = argument or "%Y-%m-%d"
	local shift, rest = format:match("^([+-]%d+y?)%s*(.*)$")
	if shift then
		t = date.zoned(date.moved(t, shift))
		format = rest ~= "" and rest or "%Y-%m-%d"
	end
	return date.format(t, format)
end
