local daiji = functions["daiji"]

test("counts by its units in daiji", function()
	eq(daiji("12"), "壱拾弐")
	eq(daiji("2026"), "弐千弐拾六")
	eq(daiji("1111"), "壱千壱百壱拾壱")
end)

test("leaves out the leading zeros and writes zero as 〇", function()
	eq(daiji("007"), "七")
	eq(daiji("0"), "〇")
end)

test("has nothing for ten thousand kei and above", function()
	eq(daiji("100000000000000000000"), nil)
end)

test("gives nil for what is no number", function()
	eq(daiji("きょう"), nil)
end)
