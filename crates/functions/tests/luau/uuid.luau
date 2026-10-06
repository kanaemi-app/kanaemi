local uuid = functions["uuid"]

test("is a version 4 UUID in lowercase", function()
	for _ = 1, 100 do
		local id = uuid("")
		eq(id:match("^%x%x%x%x%x%x%x%x%-%x%x%x%x%-4%x%x%x%-[89ab]%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x$") ~= nil, true)
		eq(id, id:lower())
	end
end)

test("is another each time", function()
	eq(uuid("") ~= uuid(""), true)
end)
