local bytes = kanaemi.random.bytes

test("gives as many bytes as asked", function()
	eq(#bytes(16), 16)
	eq(#bytes(0), 0)
	eq(#bytes(1024), 1024)
end)

test("gives other bytes each time", function()
	eq(bytes(16) ~= bytes(16), true)
end)

test("gives every value of a byte", function()
	local seen = {}
	local count = 0
	local many = bytes(1024)
	for at = 1, #many do
		local byte = string.byte(many, at)
		if not seen[byte] then
			seen[byte] = true
			count += 1
		end
	end
	eq(count > 200, true)
end)

test("fails for a count it cannot give", function()
	eq(pcall(bytes, -1), false)
	eq(pcall(bytes, 1025), false)
	eq(pcall(bytes, 1.5), false)
end)
