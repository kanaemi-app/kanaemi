local number = require("./lib/number")

test("significant drops the leading zeros and keeps one zero of zero", function()
	eq(table.concat(number.significant({ 0, 0, 7 })), "7")
	eq(table.concat(number.significant({ 0, 0, 0 })), "0")
	eq(table.concat(number.significant({ 1, 0 })), "10")
end)

test("each writes each digit as the table has it, from 0", function()
	eq(number.each({ 0, 1, 2 }, { "a", "b", "c" }), "abc")
	eq(number.each({ 2, 0, 2, 6 }, number.KANJI), "二〇二六")
	eq(number.each({ 1, 2 }, number.WIDE), "１２")
	eq(number.each({ 0, 7 }, number.ASCII), "07")
end)
