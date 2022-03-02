-- ### Configuration ###

-- Number of columns and rows
local column_count = 8
local row_count = 8

-- ### Content ###

local parameters = {
    {
        index = 0,
        name = "Column offset",
    },
    {
        index = 1,
        name = "Row offset",
    },
}

local groups = {
    {
        id = "slot-play",
        name = "Slot play",
    },
}

local mappings = {
}

-- For each column
for col = 0, column_count - 1 do
    local human_col = col + 1
    for row = 0, row_count - 1 do
        local human_row = row + 1
        local prefix = "col" .. human_col .. "/row" .. human_row .. "/"
        local slot_column_expression = "p[0] * 10000 + " .. col
        local slot_row_expression = "p[1] * 10000 + " .. row
        local slot_play = {
            id = prefix .. "slot-play",
            name = "Slot " .. human_col .. "/" .. human_row .. " play",
            group = "slot-play",
            source = {
                kind = "Virtual",
                character = "Button",
                id = prefix .. "pad",
            },
            glue = {
                absolute_mode = "ToggleButton",
            },
            target = {
                kind = "ClipTransportAction",
                slot = {
                    address = "Dynamic",
                    column_expression = slot_column_expression,
                    row_expression = slot_row_expression
                },
                action = "PlayStop",
            },
        }
        table.insert(mappings, slot_play)
    end
end

return {
    kind = "MainCompartment",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}