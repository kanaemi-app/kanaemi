local choice = functions["choice"]

test("is one of the words of the argument", function()
	local seen = {}
	for _ = 1, 200 do
		local word = choice("", "表 裏")
		eq(word == "表" or word == "裏", true)
		seen[word] = true
	end
	eq(seen["表"] and seen["裏"], true)
	eq(choice("", "  ひとつ  "), "ひとつ")
end)

test("is nothing without words", function()
	eq(choice("", nil), nil)
	eq(choice("", "   "), nil)
end)
