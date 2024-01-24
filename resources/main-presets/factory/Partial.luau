--!strict

type ArbitraryTable = { [any]: any }

--- Returns a new table that is the result of merging t2 into t1.
---
--- Values in t2 have precedence.
---
--- The result will be mergeable as well. This is good for "modifier chaining".
---
--- Typing not ideal because of https://github.com/luau-lang/luau/issues/392#issuecomment-1050344334
function merged<A, B>(t1: A, t2: B): A & B
    if type(t1) ~= "table" or type(t2) ~= "table" then
        return t1 :: any
    end
	local result = t1.clone()
	for key, new_value in pairs(t2 :: ArbitraryTable) do
		local old_value = result[key]
		if old_value and type(old_value) == "table" and type(new_value) == "table" then
			-- Merge table value as well
			result[key] = merged(old_value, new_value) :: any
		else
			-- Simple use new value
			result[key] = new_value
		end
	end
	return result :: any
end

type PartialImpl = {
    new: (part: ArbitraryTable) -> Partial,
    __index: PartialImpl,
    __add: (self: Partial, part: Partial) -> Partial,
}

export type Partial = typeof(setmetatable({}, {} :: PartialImpl))

local Partial: PartialImpl = {} :: PartialImpl
Partial.__index = Partial

function Partial.new(part)
    return setmetatable(part, Partial)
end

function Partial:__add(part)
    return Partial.new(merged(self, part) :: any)
end

return {
	Partial = Partial,
}