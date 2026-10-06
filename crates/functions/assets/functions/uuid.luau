-- UUID（第 4 版）を作る
-- uuid: a random UUID, version 4, in lowercase. Its bits come from
-- kanaemi.random, as one must never repeat.
return function()
	local bytes = { string.byte(kanaemi.random.bytes(16), 1, 16) }
	bytes[7] = bit32.bor(bit32.band(bytes[7], 0x0F), 0x40)
	bytes[9] = bit32.bor(bit32.band(bytes[9], 0x3F), 0x80)
	local out = {}
	for at, byte in ipairs(bytes) do
		table.insert(out, string.format("%02x", byte))
		if at == 4 or at == 6 or at == 8 or at == 10 then
			table.insert(out, "-")
		end
	end
	return table.concat(out)
end
