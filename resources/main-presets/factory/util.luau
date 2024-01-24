--!strict

local realearn = require("realearn")
local partial = require("partial")

local util = {}

--- Creates a partial mapping.
---
--- A partial mapping can be conveniently mixed with other partial mappings by using the + operator.
--- This enables a very intuitive but still flexible way of writing mappings.
function util.partial_mapping(m: realearn.Mapping): partial.Partial
	return partial.Partial.new(m)
end

--- Returns the values of the given key-value table as array.
function util.to_array<V>(t: { [string]: V }): { V }
	local array = {}
	for _, v in t do
		table.insert(array, v)
	end
	return array
end

--- Returns the values of the given key-value table as array, sorted by the values'
--- `index` key.
function util.sorted_by_index<V>(t: { [string]: V }): { V }
	local sorted = util.to_array(t)
	local compare_index = function(left: any, right: any): boolean
		return left.index < right.index
	end
	table.sort(sorted, compare_index)
	return sorted
end

--- Takes a key-value table and adds a new attribute `id` to each value that
--- corresponds to the key.
function util.set_keys_as_ids(t: { [string]: { [string]: any } })
	for key, value in pairs(t) do
		value.id = key
	end
end

--- Puts each `label` property value of the given array of tables into a new array.
function util.extract_labels(array: { { [string]: any } }): { string }
	local labels = {}
	for _, element in ipairs(array) do
		table.insert(labels, element.label)
	end
	return labels
end

return util
