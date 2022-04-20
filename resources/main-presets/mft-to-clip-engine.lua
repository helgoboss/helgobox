-- Preparation

local col_param = 0
local row_param = 1

function create_cell_id(col, row, id)
    return "col" .. (col + 1) .. "/row" .. (row + 1) .. "/" .. id
end

function create_col_expression(col)
    return "p[0] + " .. col
end

function create_row_expression(row)
    return "p[1] + " .. row
end

function clip_play(col, row)
    return {
        group = "clip-play",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            character = "Button",
            id = create_cell_id(col, row, "pad"),
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "ClipTransportAction",
            slot = {
                address = "Dynamic",
                column_expression = create_col_expression(col),
                row_expression = create_row_expression(row)
            },
            action = "RecordPlayStop",
            record_only_if_track_armed = true,
            stop_column_if_slot_empty = true,
        },
    }
end

function clip_volume(col, row)
    return {
        group = "clip-volume",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            character = "Multi",
            id = create_cell_id(col, row, "knob"),
        },
        target = {
            kind = "ClipVolume",
            slot = {
                address = "Dynamic",
                column_expression = create_col_expression(col),
                row_expression = create_row_expression(row)
            },
        },
    }
end

function slot_state_feedback(col, row)
    return {
        group = "slot-state-feedback",
        control_enabled = false,
        source = {
            kind = "Virtual",
            character = "Button",
            id = create_cell_id(col, row, "pad"),
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
                column_expression = create_col_expression(col),
                row_expression = create_row_expression(row)
            },
            action = "PlayStop",
        },
    }
end

function clip_position_feedback(col, row)
    return {
        group = "clip-position-feedback",
        control_enabled = false,
        source = {
            kind = "Virtual",
            character = "Multi",
            id = create_cell_id(col, row, "knob"),
        },
        target = {
            kind = "ClipSeek",
            slot = {
                address = "Dynamic",
                column_expression = create_col_expression(col),
                row_expression = create_row_expression(row),
            },
            feedback_resolution = "High",
        },
    }
end

function inc_button(button_id, param_index, amount)
    local amount_abs = math.abs(amount)
    return {
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = button_id,
            character = "Button",
        },
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = amount < 0,
            step_factor_interval = { amount_abs, amount_abs }
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = param_index,
            },
        },
    }
end

-- Content

local mappings = {
    inc_button("bank-left", col_param, -1),
    inc_button("bank-right", col_param, 1),
    inc_button("ch-left", row_param, -1),
    inc_button("ch-right", row_param, 1),
}

local groups = {
    {
        id = "clip-play",
        name = "Clip play",
    },
    {
        id = "clip-volume",
        name = "Clip volume",
    },
    {
        id = "slot-state-feedback",
        name = "Slot state feedback",
    },
    {
        id = "clip-position-feedback",
        name = "Clip position feedback",
    },
}

local parameters = {
    {
        index = col_param,
        name = "Column offset",
        value_count = 10000,
    },
    {
        index = row_param,
        name = "Row offset",
        value_count = 10000,
    },
}

-- Grid

local column_count = 4
local row_count = 4
for col = 0, column_count - 1 do
    for row = 0, row_count - 1 do
        table.insert(mappings, clip_play(col, row))
        table.insert(mappings, clip_volume(col, row))
        table.insert(mappings, slot_state_feedback(col, row))
        table.insert(mappings, clip_position_feedback(col, row))
    end
end

-- Result

return {
    kind = "MainCompartment",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}