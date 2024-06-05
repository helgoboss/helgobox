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
        label = "Global",
        button = "bank-left",
    },
    -- OK Solo/arm/mute/select column tracks
    track = {
        index = 2,
        label = "Track",
        button = "cursor-left",
    },
    -- OK Delete clip (long press)
    -- OK Quantize clip (double press)
    slot = {
        index = 3,
        label = "Slot",
        button = "ch-left",
    },
    -- OK Scroll horizontally
    -- OK Scroll vertically
    nav = {
        index = 4,
        label = "Navigation",
        button = "bank-right",
    },
    -- OK Stop column
    column = {
        index = 5,
        label = "Column",
        button = "cursor-right",
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
        name = modes.global.label,
        activation_condition = mode_is(modes.global.index),
    },
    nav = {
        name = modes.nav.label,
        activation_condition = mode_is(modes.nav.index),
    },
    track = {
        name = modes.track.label,
        activation_condition = mode_is(modes.track.index),
    },
    slot = {
        name = modes.slot.label,
        activation_condition = mode_is(modes.slot.index),
    },
    column = {
        name = modes.column.label,
        activation_condition = mode_is(modes.column.index),
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
        name = "Rec/play",
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
        name = "Vol",
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
        visible_in_projection = false,
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
        visible_in_projection = false,
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

function global_matrix_button(button_id, matrix_action, name)
    return {
        name = name or matrix_action,
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

function global_reaper_action_button(button_id, command_id, name)
    return {
        name = name,
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

function scroll(multi_id, offset_param_index, name)
    return {
        name = name,
        group = groups.nav.id,
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
        name = "Quantize/delete",
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

function track_toggle_target(col, row, target_kind, name, exclusive)
    return {
        name = name,
        group = groups.track.id,
        source = {
            kind = "Virtual",
            character = "Button",
            id = create_cell_id(col, row, "pad"),
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = target_kind,
            track = {
                address = "FromClipColumn",
                column = {
                    address = "Dynamic",
                    expression = create_col_expression(col),
                },
                context = "Playback",
            },
            exclusivity = exclusive and "WithinFolderOnOnly",
        },
    }
end

function column_action(col, row, action)
    return {
        name = action,
        group = groups.column.id,
        source = {
            kind = "Virtual",
            character = "Button",
            id = create_cell_id(col, row, "pad"),
        },
        target = {
            kind = "ClipColumnAction",
            column = {
                address = "Dynamic",
                expression = create_col_expression(col),
            },
            action = action
        },
    }
end

function clip_quantize(col, row)
    return {
        group = groups.slot.id,
        visible_in_projection = false,
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

-- TODO-high-playtime-after-release Make short press toggle and long press be momentary.
--  Problem 1: "Fire after timeout" somehow doesn't have an effect.
--  Problem 2: "Fire after timeout" doesn't switch off when button released (in new versions it does!)
function mode_button(button_id, mode)
    local target_value = mode.index / (mode_count - 1)
    return {
        name = mode.label,
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
    global_matrix_button("col4/row4/pad", "Stop", "Stop all clips"),
    global_matrix_button("col3/row1/pad", "Undo"),
    global_matrix_button("col4/row1/pad", "Redo"),
    global_transport_button("col4/row3/pad", "PlayPause", "ToggleButton"),
    global_transport_button("col3/row3/pad", "Stop", "Normal"),
    global_reaper_action_button("col4/row2/pad", 40364, "Click"),
    scroll("col1/row1/knob", params.column_offset.index, "Scroll left/right"),
    scroll("col1/row2/knob", params.row_offset.index, "Scroll up/down"),
}

-- Mode buttons

for _, mode in pairs(modes) do
    if mode.button then
        table.insert(mappings, mode_button(mode.button, mode))
    end
end

-- Grid

for col = 0, column_count - 1 do
    table.insert(mappings, track_toggle_target(col, 0, "TrackSoloState", "Solo"))
    table.insert(mappings, track_toggle_target(col, 1, "TrackArmState", "Arm", true))
    table.insert(mappings, track_toggle_target(col, 2, "TrackMuteState", "Mute"))
    table.insert(mappings, track_toggle_target(col, 3, "TrackSelectionState", "Select", true))
    table.insert(mappings, column_action(col, 3, "Stop"))
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