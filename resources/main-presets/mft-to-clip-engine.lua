-- ### Preparation ###

-- Constants

local column_count = 4
local row_count = 4
local mode_count = 100

-- Utilities

function set_keys_as_ids(t)
    for key, value in pairs(t) do
        value.id = key
    end
end

function to_array(t)
    local array = {}
    for _, v in pairs(t) do
        table.insert(array, v)
    end
    return array
end

function sorted_by_index(t)
    local sorted = to_array(t)
    local compare_index = function(left, right)
        return left.index < right.index
    end
    table.sort(sorted, compare_index)
    return sorted
end

-- Modes

local modes = {
    -- OK Record/play/stop clips (push)
    -- OK Set clip volume (turn)
    normal = {
        index = 0,
        label = "Normal",
    },
    -- OK Undo/redo
    -- OK Stop all
    -- OK Play
    -- OK Click
    -- Cycle through normal mode knob functions (vol, pan, send)
    global = {
        index = 1,
        label = "Global functions",
        button = "bank-left",
    },
    -- Solo/arm/mute/select column tracks
    track = {
        index = 2,
        label = "Column track functions",
        button = "cursor-left",
    },
    -- OK Delete clip (long press)
    -- OK Quantize clip (double press)
    slot = {
        index = 3,
        label = "Slot functions",
        button = "ch-left",
    },
    -- OK Scroll horizontally
    -- OK Scroll vertically
    global_nav = {
        index = 4,
        label = "Global navigation functions",
        button = "bank-right",
    },
}
local sorted_modes = sorted_by_index(modes)
local mode_labels = {}
for _, mode in ipairs(sorted_modes) do
    table.insert(mode_labels, mode.label)
end

-- Parameters

local params = {
    column_offset = {
        index = 0,
        name = "Column offset",
        value_count = 10000,
    },
    row_offset = {
        index = 1,
        name = "Row offset",
        value_count = 10000,
    },
    mode = {
        index = 2,
        name = "Mode",
        value_count = mode_count,
        value_labels = mode_labels,
    },
}
-- Groups

function mode_is(mode_index)
    return {
        kind = "Bank",
        parameter = params.mode.index,
        bank_index = mode_index,
    }
end

function display_slot_feedback_condition()
    return {
        kind = "Expression",
        condition = "p[2] == 0 || p[2] == 4 || p[2] == 3",
    }
end

local groups = {
    slot_state_feedback = {
        name = "Slot state feedback",
        activation_condition = display_slot_feedback_condition(),
    },
    clip_play = {
        name = "Clip play",
        activation_condition = mode_is(modes.normal.index),
    },
    clip_volume = {
        name = "Clip volume",
        activation_condition = mode_is(modes.normal.index),
    },
    clip_pos_feedback = {
        name = "Clip position feedback",
        activation_condition = display_slot_feedback_condition(),
    },
    global = {
        name = "Global functions",
        activation_condition = mode_is(modes.global.index),
    },
    global_nav = {
        name = "Global navigation functions",
        activation_condition = mode_is(modes.global_nav.index),
    },
    track = {
        name = "Track functions",
        activation_condition = mode_is(modes.track.index),
    },
    slot = {
        name = "Slot functions",
        activation_condition = mode_is(modes.slot.index),
    },
}
set_keys_as_ids(groups)

-- Domain functions

function create_cell_id(col, row, id)
    return "col" .. (col + 1) .. "/row" .. (row + 1) .. "/" .. id
end

function create_coordinate_expression(param, index)
    return "p[" .. param .. "] + " .. index
end

function create_col_expression(col)
    return create_coordinate_expression(params.column_offset.index, col)
end

function create_row_expression(row)
    return create_coordinate_expression(params.row_offset.index, row)
end

function create_slot_selector(col, row)
    return {
        address = "Dynamic",
        column_expression = create_col_expression(col),
        row_expression = create_row_expression(row)
    }
end

function clip_play(col, row)
    return {
        group = groups.clip_play.id,
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
            slot = create_slot_selector(col, row),
            action = "RecordPlayStop",
            record_only_if_track_armed = true,
            stop_column_if_slot_empty = true,
        },
    }
end

function clip_volume(col, row)
    return {
        group = groups.clip_volume.id,
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            character = "Multi",
            id = create_cell_id(col, row, "knob"),
        },
        target = {
            kind = "ClipVolume",
            slot = create_slot_selector(col, row),
        },
    }
end

function slot_state_feedback(col, row)
    return {
        group = groups.slot_state_feedback.id,
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
            slot = create_slot_selector(col, row),
            action = "PlayStop",
        },
    }
end

function clip_position_feedback(col, row)
    return {
        group = groups.clip_pos_feedback.id,
        control_enabled = false,
        source = {
            kind = "Virtual",
            character = "Multi",
            id = create_cell_id(col, row, "knob"),
        },
        target = {
            kind = "ClipSeek",
            slot = create_slot_selector(col, row),
            feedback_resolution = "High",
        },
    }
end

function global_matrix_button(button_id, matrix_action)
    return {
        activation_condition = mode_is(modes.global.index),
        source = {
            kind = "Virtual",
            id = button_id,
            character = "Button",
        },
        target = {
            kind = "ClipMatrixAction",
            action = matrix_action,
        },
    }
end

function global_transport_button(button_id, transport_action, absolute_mode)
    return {
        activation_condition = mode_is(modes.global.index),
        source = {
            kind = "Virtual",
            id = button_id,
            character = "Button",
        },
        glue = {
            absolute_mode = absolute_mode,
        },
        target = {
            kind = "TransportAction",
            action = transport_action,
        },
    }
end

function global_reaper_action_button(button_id, command_id)
    return {
        activation_condition = mode_is(modes.global.index),
        source = {
            kind = "Virtual",
            id = button_id,
            character = "Button",
        },
        target = {
            kind = "ReaperAction",
            command = command_id,
            invocation = "Trigger",
        },
    }
end

function scroll(multi_id, offset_param_index)
    return {
        group = groups.global_nav.id,
        source = {
            kind = "Virtual",
            id = multi_id,
            character = "Multi",
        },
        glue = {
            step_factor_interval = { -3, -3 },
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = offset_param_index,
            },
        },
    }
end

function clip_delete(col, row)
    return {
        group = groups.slot.id,
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            character = "Button",
            id = create_cell_id(col, row, "pad"),
        },
        glue = {
            fire_mode = {
                kind = "AfterTimeout",
                timeout = 1000,
            },
        },
        target = {
            kind = "ClipManagement",
            slot = create_slot_selector(col, row),
            action = {
                kind = "ClearSlot",
            },
        },
    }
end

function clip_quantize(col, row)
    return {
        group = groups.slot.id,
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            character = "Button",
            id = create_cell_id(col, row, "pad"),
        },
        glue = {
            absolute_mode = "ToggleButton",
            fire_mode = {
                kind = "OnDoublePress",
            },
        },
        target = {
            kind = "ClipManagement",
            slot = create_slot_selector(col, row),
            action = {
                kind = "EditClip",
            },
        },
    }
end

-- TODO-high Make short press toggle and long press be momentary.
--  Problem 1: "Fire after timeout" somehow doesn't have an effect.
--  Problem 2: "Fire after timeout" doesn't switch off when button released.
function mode_button(button_id, mode_index)
    local target_value = mode_index / (mode_count - 1)
    return {
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            id = button_id,
            character = "Button",
        },
        glue = {
            target_interval = { 0, target_value }
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = params.mode.index,
            },
        },
    }
end

-- Content

local mappings = {
    global_matrix_button("col4/row4/pad", "Stop"),
    global_matrix_button("col3/row1/pad", "Undo"),
    global_matrix_button("col4/row1/pad", "Redo"),
    global_transport_button("col4/row3/pad", "PlayPause", "ToggleButton"),
    global_transport_button("col3/row3/pad", "Stop", "Normal"),
    global_reaper_action_button("col4/row2/pad", 40364),
    scroll("col1/row1/knob", params.row_offset.index),
    scroll("col1/row2/knob", params.column_offset.index),
}

-- Mode buttons

for _, mode in pairs(modes) do
    if mode.button then
        table.insert(mappings, mode_button(mode.button, mode.index))
    end
end

-- Grid

for col = 0, column_count - 1 do
    for row = 0, row_count - 1 do
        table.insert(mappings, clip_play(col, row))
        table.insert(mappings, clip_volume(col, row))
        table.insert(mappings, clip_delete(col, row))
        table.insert(mappings, clip_quantize(col, row))
        table.insert(mappings, slot_state_feedback(col, row))
        table.insert(mappings, clip_position_feedback(col, row))
    end
end

-- Result

return {
    kind = "MainCompartment",
    value = {
        parameters = sorted_by_index(params),
        groups = to_array(groups),
        mappings = mappings,
    },
}