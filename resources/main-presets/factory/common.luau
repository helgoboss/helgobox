--!strict
local common = {}

export type ArbitraryTable = { [any]: any }

--- Creates a shallow clone of a table.
---
--- Typing not ideal because of https://github.com/luau-lang/luau/issues/392#issuecomment-1050344334
function common.clone<T>(t: T): T
    if type(t) ~= "table" then
        return t
    end
	local new_table = {}
	for k, v in t :: ArbitraryTable do
		new_table[k] = v
	end
	return new_table :: any
end

--- Converts the given key-value table to an array.
function common.to_array<V>(t: { [string]: V }): { V }
	local array = {}
	for _, v in t do
		table.insert(array, v)
	end
	return array
end

--- Returns a new table that's the given table turned into an array
--- and sorted by the `index` key.
function common.sorted_by_index<V>(t: { [string]: V }): { V }
	local sorted = common.to_array(t)
	local compare_index = function(left: any, right: any): boolean
		return left.index < right.index
	end
	table.sort(sorted, compare_index)
	return sorted
end

return common