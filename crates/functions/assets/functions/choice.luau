-- 引数の語から 1 つを選ぶ（表 裏 → 表）
-- choice: one of the words of the argument, apart by spaces, at random.
return function(source, argument)
	local words = {}
	for word in (argument or ""):gmatch("%S+") do
		table.insert(words, word)
	end
	if #words == 0 then
		return nil
	end
	return words[math.random(1, #words)]
end
