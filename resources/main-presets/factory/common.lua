local common = {}

--- Converts the given key-value table to an array table.
function common.to_array(t)
    local array = {}
    for _, v in pairs(t) do
        table.insert(array, v)
    end
    return array
end

--- Returns a new table that's the given table turned into an array
--- and sorted by the `index` key.
function common.sorted_by_index(t)
    local sorted = common.to_array(t)
    local compare_index = function(left, right)
        return left.index < right.index
    end
    table.sort(sorted, compare_index)
    return sorted
end

--- Clones a table.
function common.clone(t)
    local new_table = {}
    for k, v in pairs(t) do
        new_table[k] = v
    end
    return new_table
end

--- Returns a new table that is the result of merging t2 into t1.
---
--- Values in t2 have precedence.
---
--- The result will be mergeable as well. This is good for "modifier chaining".
function common.merged(t1, t2)
    local result = common.clone(t1)
    for key, new_value in pairs(t2) do
        local old_value = result[key]
        if old_value and type(old_value) == "table" and type(new_value) == "table" then
            -- Merge table value as well
            result[key] = common.merged(old_value, new_value)
        else
            -- Simple use new value
            result[key] = new_value
        end
    end
    return common.make_mergeable(result)
end


--- Makes it possible to merge this table with another one via "+" operator.
function common.make_mergeable(t)
    local metatable = {
        __add = common.merged
    }
    setmetatable(t, metatable)
    return t
end

return common
