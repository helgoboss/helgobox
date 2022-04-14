-- ### Configuration ###

-- Modes
local mode_count = 100
local modes = {
    { label = "Normal" },
    { label = "Record", button = "record" },
    { label = "Delete", button = "delete" },
    { label = "Quantize", button = "quantize" },
}

-- Number of columns and rows
local column_count = 8
local row_count = 8

-- ### Content ###

local mappings = {}
local mode_labels = {}

for i, mode in ipairs(modes) do
    table.insert(mode_labels, mode.label)
    if mode.button then
        local target_value = (i - 1) / mode_count
        local m = {
            group = "modes",
            name = mode.label,
            source = {
                kind = "Virtual",
                id = mode.button,
                character = "Button",
            },
            glue = {
                absolute_mode = "ToggleButton",
                target_interval = { target_value, target_value },
                out_of_range_behavior = "Min",
            },
            target = {
                kind = "FxParameterValue",
                parameter = {
                    address = "ById",
                    index = 3,
                },
            },
        }
        table.insert(mappings, m)
    end
end

local parameters = {
    {
        index = 0,
        name = "Column offset",
        value_count = 10000,
    },
    {
        index = 1,
        name = "Row offset",
        value_count = 10000,
    },
    {
        index = 2,
        name = "Shift modifier",
    },
    {
        index = 3,
        name = "Mode",
        value_count = mode_count,
        value_labels = mode_labels
    },
}

local groups = {
    {
        id = "modes",
        name = "Modes",
    },
    {
        id = "slot-play",
        name = "Slot play",
    },
    {
        id = "slot-record",
        name = "Slot record",
    },
    {
        id = "slot-clear",
        name = "Slot clear",
    },
    {
        id = "slot-quantize",
        name = "Slot quantize",
    },
    {
        id = "column-stop",
        name = "Column stop",
    },
}

-- For each column
for col = 0, column_count - 1 do
    local human_col = col + 1
    local prefix = "col" .. human_col .. "/"
    local column_expression = "p[0] + " .. col
    local column_stop = {
        name = "Column " .. human_col .. " stop",
        group = "column-stop",
        source = {
            kind = "Virtual",
            character = "Button",
            id = prefix .. "stop",
        },
        target = {
            kind = "ClipColumnTransportAction",
            column = {
                address = "Dynamic",
                expression = column_expression,
            },
        },
    }
    table.insert(mappings, column_stop)
end

-- For each slot
for col = 0, column_count - 1 do
    local human_col = col + 1
    for row = 0, row_count - 1 do
        local human_row = row + 1
        local prefix = "col" .. human_col .. "/row" .. human_row .. "/"
        local slot_column_expression = "p[0] + " .. col
        local slot_row_expression = "p[1] + " .. row
        local slot_play = {
            id = prefix .. "slot-play",
            name = "Slot " .. human_col .. "/" .. human_row .. " play",
            group = "slot-play",
            feedback_enabled = false,
            activation_condition = {
                kind = "Bank",
                parameter = 3,
                bank_index = 0,
            },
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
        local slot_play_feedback = {
            id = prefix .. "slot-play-feedback",
            name = "Slot " .. human_col .. "/" .. human_row .. " play feedback",
            group = "slot-play",
            control_enabled = false,
            source = {
                kind = "Virtual",
                character = "Button",
                id = prefix .. "pad",
            },
            glue = {
                feedback = {
                    kind = "Text",
                    text_expression = "{{ target.slot_state.id }}",
                },
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
        local slot_record = {
            id = prefix .. "slot-record",
            name = "Slot " .. human_col .. "/" .. human_row .. " record",
            group = "slot-record",
            feedback_enabled = false,
            activation_condition = {
                kind = "Bank",
                parameter = 3,
                bank_index = 1,
            },
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
                action = "Record",
            },
        }
        local slot_clear = {
            id = prefix .. "slot-clear",
            name = "Slot " .. human_col .. "/" .. human_row .. " clear",
            group = "slot-clear",
            feedback_enabled = false,
            activation_condition = {
                kind = "Bank",
                parameter = 3,
                bank_index = 2,
            },
            source = {
                kind = "Virtual",
                character = "Button",
                id = prefix .. "pad",
            },
            target = {
                kind = "ClipManagement",
                slot = {
                    address = "Dynamic",
                    column_expression = slot_column_expression,
                    row_expression = slot_row_expression
                },
                action = {
                    kind = "ClearSlot",
                },
            },
        }
        local slot_quantize = {
            id = prefix .. "slot-quantize",
            name = "Slot " .. human_col .. "/" .. human_row .. " quantize",
            group = "slot-quantize",
            feedback_enabled = false,
            activation_condition = {
                kind = "Modifier",
                kind = "Bank",
                parameter = 3,
                bank_index = 4,
            },
            source = {
                kind = "Virtual",
                character = "Button",
                id = prefix .. "pad",
            },
            glue = {
                absolute_mode = "ToggleButton",
            },
            target = {
                kind = "ClipManagement",
                slot = {
                    address = "Dynamic",
                    column_expression = slot_column_expression,
                    row_expression = slot_row_expression
                },
                action = {
                    kind = "EditClip",
                },
            },
        }
        table.insert(mappings, slot_play)
        table.insert(mappings, slot_play_feedback)
        table.insert(mappings, slot_record)
        table.insert(mappings, slot_clear)
        table.insert(mappings, slot_quantize)
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