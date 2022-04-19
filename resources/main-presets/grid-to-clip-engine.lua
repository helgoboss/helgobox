-- ### Configuration ###

-- Device type
--local device = "generic"
local device = "apc_key_25"
--local device = "launchpad_pro_mk2"

-- Slot modes
local slot_mode_count = 100
local slot_modes = {
    { label = "Normal" },
    { label = "Delete", button = "delete" },
    { label = "Quantize", button = "quantize" },
}

-- Column modes
local column_mode_count = 100
local column_modes = {
    {
        id = "stop",
        label = "Stop clip",
        button = "stop_clip",
    },
    {
        id = "solo",
        label = "Solo",
        button = "solo",
        track_target = "TrackSoloState",
    },
    {
        id = "record-arm",
        label = "Record arm",
        button = "record_arm",
        track_target = "TrackArmState",
    },
    {
        id = "mute",
        label = "Mute",
        button = "mute",
        track_target = "TrackMuteState",
    },
    {
        id = "select",
        label = "Track select",
        button = "track_select",
        track_target = "TrackSelectionState",
    },

}

-- Knob modes
local knob_mode_count = 100
local knob_modes = {
    {
        id = "volume",
        label = "Volume",
        button = "volume",
        track_target = "TrackVolume",
    },
    {
        id = "pan",
        label = "Pan",
        button = "pan",
        track_target = "TrackPan",
    },
    {
        id = "sends",
        label = "Sends",
        button = "sends"
    },
    {
        id = "device",
        label = "Device",
        button = "device"
    },
}

-- Number of columns and rows
-- TODO-medium Would be good to take this dynamically from the controller preset as a compartment variable.
--- However, at the moment it's not relevant. We just take a reasonable maximum.
local column_count = 8
local row_count = 8

-- ### Content ###

-- Common functions

function merged(t1, t2)
    local new_table = {}
    for k, v in pairs(t1) do
        new_table[k] = v
    end
    for k, v in pairs(t2) do
        new_table[k] = v
    end
    return new_table
end

function make_mergeable(t)
    local metatable = {
        __add = merged
    }
    setmetatable(t, metatable)
end

-- Device-specific resolutions

function button(id)
    return {
        source = {
            kind = "Virtual",
            id = id,
            character = "Button",
        },
    }
end

function shift_pressed(state)
    local partial_mapping = {
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 2,
                    on = state,
                },
            },
        },
    }
    make_mergeable(partial_mapping)
    return partial_mapping
end

local shift = shift_pressed(true)
local not_shift = shift_pressed(false)

local device_specific = {
    generic = {
        cursor_up = button("cursor-up"),
        cursor_down = button("cursor-down"),
        cursor_left = button("cursor-left"),
        cursor_right = button("cursor-right"),
        volume = button("volume"),
        pan = button("pan"),
        sends = button("sends"),
        device = button("device"),
        stop_clip = button("stop-clip"),
        solo = button("solo"),
        record_arm = button("record-arm"),
        mute = button("mute"),
        track_select = button("track-select"),
        normal_function = {},
    },
    apc_key_25 = {
        cursor_up = shift + button("col1/stop"),
        cursor_down = shift + button("col2/stop"),
        cursor_left = shift + button("col3/stop"),
        cursor_right = shift + button("col4/stop"),
        volume = shift + button("col5/stop"),
        pan = shift + button("col6/stop"),
        sends = shift + button("col7/stop"),
        device = shift + button("col8/stop"),
        stop_clip = button("row1/play"),
        solo = button("row2/play"),
        record_arm = button("row3/play"),
        mute = button("row4/play"),
        track_select = button("row5/play"),
        normal_function = not_shift,
    },
}

for _, t1 in pairs(device_specific) do
    for _, t2 in pairs(t1) do
        make_mergeable(t2)
    end
end

-- Global mappings

local mappings = {
    {
        name = "Stop all clips",
        source = {
            kind = "Virtual",
            id = "stop-all-clips",
            character = "Button",
        },
        target = {
            kind = "ClipMatrixAction",
            action = "Stop",
        },
    },
    {
        name = "Play arrangement",
        source = {
            kind = "Virtual",
            id = "play",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TransportAction",
            action = "PlayPause",
        },
    },
    device_specific[device].cursor_up + {
        name = "Scroll up",
        feedback_enabled = false,
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
    device_specific[device].cursor_down + {
        name = "Scroll down",
        feedback_enabled = false,
        glue = {
            absolute_mode = "IncrementalButton",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 1,
            },
        },
    },
    device_specific[device].cursor_left + {
        name = "Scroll left",
        feedback_enabled = false,
        glue = {
            absolute_mode = "IncrementalButton",
            reverse = true,
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    device_specific[device].cursor_right + {
        name = "Scroll right",
        feedback_enabled = false,
        glue = {
            absolute_mode = "IncrementalButton",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 0,
            },
        },
    },
    {
        name = "Shift",
        source = {
            kind = "Virtual",
            id = "shift",
            character = "Button",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 2,
            },
        },
    },
}

-- Slot modes
local slot_mode_labels = {}
for i, mode in ipairs(slot_modes) do
    table.insert(slot_mode_labels, mode.label)
    if mode.button then
        local target_value = (i - 1) / slot_mode_count
        local m = {
            group = "slot-modes",
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

-- Column modes
local column_mode_labels = {}
for i, mode in ipairs(column_modes) do
    table.insert(column_mode_labels, mode.label)
    local target_value = (i - 1) / column_mode_count
    local m = device_specific[device][mode.button] + {
        group = "column-modes",
        name = mode.label,
        glue = {
            target_interval = { target_value, target_value },
            out_of_range_behavior = "Min",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 4,
            },
        },
    }
    table.insert(mappings, m)
end

-- Knob modes
local knob_mode_labels = {}
for i, mode in ipairs(knob_modes) do
    table.insert(knob_mode_labels, mode.label)
    local target_value = (i - 1) / knob_mode_count
    local m = device_specific[device][mode.button] + {
        group = "knob-modes",
        name = mode.label,
        glue = {
            target_interval = { target_value, target_value },
            out_of_range_behavior = "Min",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 5,
            },
        },
    }
    table.insert(mappings, m)
end

-- Parameters
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
        name = "Slot mode",
        value_count = slot_mode_count,
        value_labels = slot_mode_labels
    },
    {
        index = 4,
        name = "Column mode",
        value_count = column_mode_count,
        value_labels = column_mode_labels
    },
    {
        index = 5,
        name = "Knob mode",
        value_count = knob_mode_count,
        value_labels = knob_mode_labels
    },
}

local groups = {
    {
        id = "slot-modes",
        name = "Slot modes",
    },
    {
        id = "column-modes",
        name = "Column modes",
    },
    {
        id = "knob-modes",
        name = "Knob modes",
    },
    {
        id = "slot-feedback",
        name = "Slot feedback",
    },
    {
        id = "slot-play",
        name = "Slot play",
        activation_condition = {
            kind = "Bank",
            parameter = 3,
            bank_index = 0,
        },
    },
    {
        id = "slot-record",
        name = "Slot record",
        activation_condition = {
            kind = "Bank",
            parameter = 3,
            bank_index = 1,
        },
    },
    {
        id = "slot-clear",
        name = "Slot clear",
        activation_condition = {
            kind = "Bank",
            parameter = 3,
            bank_index = 2,
        },
    },
    {
        id = "slot-quantize",
        name = "Slot quantize",
        activation_condition = {
            kind = "Bank",
            parameter = 3,
            bank_index = 3,
        },
    },
    {
        id = "column-stop",
        name = "Column stop",
        activation_condition = {
            kind = "Bank",
            parameter = 4,
            bank_index = 0,
        },
    },
    {
        id = "column-solo",
        name = "Column solo",
        activation_condition = {
            kind = "Bank",
            parameter = 4,
            bank_index = 1,
        },
    },
    {
        id = "column-record-arm",
        name = "Column record arm",
        activation_condition = {
            kind = "Bank",
            parameter = 4,
            bank_index = 2,
        },
    },
    {
        id = "column-mute",
        name = "Column mute",
        activation_condition = {
            kind = "Bank",
            parameter = 4,
            bank_index = 3,
        },
    },
    {
        id = "column-select",
        name = "Column select",
        activation_condition = {
            kind = "Bank",
            parameter = 4,
            bank_index = 4,
        },
    },
    {
        id = "knob-volume",
        name = "Knob volume",
        activation_condition = {
            kind = "Bank",
            parameter = 5,
            bank_index = 0,
        },
    },
    {
        id = "knob-pan",
        name = "Knob pan",
        activation_condition = {
            kind = "Bank",
            parameter = 5,
            bank_index = 1,
        },
    },
    {
        id = "knob-sends",
        name = "Knob sends",
        activation_condition = {
            kind = "Bank",
            parameter = 5,
            bank_index = 2,
        },
    },
    {
        id = "knob-device",
        name = "Knob device",
        activation_condition = {
            kind = "Bank",
            parameter = 5,
            bank_index = 3,
        },
    },
}

-- Normal (non-shift) column-mode-dependent functions for each column
for col = 0, column_count - 1 do
    local human_col = col + 1
    local prefix = "col" .. human_col .. "/"
    local column = {
        address = "Dynamic",
        expression = "p[0] + " .. col,
    }
    -- Column button
    local column_stop_mapping = device_specific[device].normal_function + {
        name = "Column " .. human_col .. " stop",
        group = "column-stop",
        source = {
            kind = "Virtual",
            character = "Button",
            id = prefix .. "stop",
        },
        target = {
            kind = "ClipColumnAction",
            column = column,
            action = "Stop",
        },
    }
    table.insert(mappings, column_stop_mapping)
    for _, mode in ipairs(column_modes) do
        if mode.track_target then
            local mapping = device_specific[device].normal_function + {
                name = "Column " .. human_col .. " " .. mode.id,
                group = "column-" .. mode.id,
                source = {
                    kind = "Virtual",
                    character = "Button",
                    id = prefix .. "stop",
                },
                glue = {
                    absolute_mode = "ToggleButton",
                },
                target = {
                    kind = mode.track_target,
                    track = {
                        address = "FromClipColumn",
                        column = column,
                        context = "Playback",
                    },
                },
            }
            table.insert(mappings, mapping)
        end
    end
    -- Column knob
    for _, mode in ipairs(knob_modes) do
        if mode.track_target then
            local mapping = {
                name = "Knob " .. human_col .. " " .. mode.id,
                group = "knob-" .. mode.id,
                source = {
                    kind = "Virtual",
                    character = "Multi",
                    id = col,
                },
                target = {
                    kind = mode.track_target,
                    track = {
                        address = "FromClipColumn",
                        column = column,
                        context = "Playback",
                    },
                },
            }
            table.insert(mappings, mapping)
        end
    end
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
                action = "RecordPlayStop",
                record_only_if_track_armed = true,
                stop_column_if_slot_empty = true,
            },
        }
        local slot_play_feedback = {
            id = prefix .. "slot-play-feedback",
            name = "Slot " .. human_col .. "/" .. human_row .. " play feedback",
            group = "slot-feedback",
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
        local slot_clear = {
            id = prefix .. "slot-clear",
            name = "Slot " .. human_col .. "/" .. human_row .. " clear",
            group = "slot-clear",
            feedback_enabled = false,
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