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
    {
        index = 2,
        name = "Shift modifier",
    },
    {
        index = 3,
        name = "Record modifier",
    },
    {
        index = 4,
        name = "Delete modifier",
    },
    {
        index = 5,
        name = "quantize modifier",
    },
}

local groups = {
    {
        id = "exclusive-modifiers",
        name = "Exclusive modifiers",
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
}

local mappings = {
    {
        id = "record-modifier",
        group = "exclusive-modifiers",
        name = "Record modifier",
        source = {
            kind = "Virtual",
            id = "record",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            interaction = "InverseTargetValueOnOnly",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 3,
            },
        },
    },
    {
        id = "delete-modifier",
        group = "exclusive-modifiers",
        name = "Delete modifier",
        source = {
            kind = "Virtual",
            id = "delete",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            interaction = "InverseTargetValueOnOnly",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 4,
            },
        },
    },
    {
        id = "quantize-modifier",
        group = "exclusive-modifiers",
        name = "quantize modifier",
        source = {
            kind = "Virtual",
            id = "quantize",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
            interaction = "InverseTargetValueOnOnly",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 5,
            },
        },
    },
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
            feedback_enabled = false,
            activation_condition = {
                kind = "Modifier",
                modifiers = {
                    {
                        parameter = 3,
                        on = false,
                    },
                    {
                        parameter = 4,
                        on = false,
                    },
                },
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
                kind = "Modifier",
                modifiers = {
                    {
                        parameter = 3,
                        on = true,
                    },
                },
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
                kind = "Modifier",
                modifiers = {
                    {
                        parameter = 4,
                        on = true,
                    },
                },
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
                modifiers = {
                    {
                        parameter = 5,
                        on = true,
                    },
                },
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