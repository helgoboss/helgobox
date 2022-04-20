-- ### Configuration ###

-- Device type
local device = "generic"
--local device = "apc_key_25"
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

-- Clones a table.
function clone(t)
    local new_table = {}
    for k, v in pairs(t) do
        new_table[k] = v
    end
    return new_table
end

-- Returns a new table that is the result of merging t2 into t1.
--
-- Values in t2 have precedence.
--
-- The result will be mergeable as well. This is good for "modifier chaining".
function merged(t1, t2)
    local result = clone(t1)
    for key, new_value in pairs(t2) do
        local old_value = result[key]
        if old_value and type(old_value) == "table" and type(new_value) == "table" then
            -- Merge table value as well
            result[key] = merged(old_value, new_value)
        else
            -- Simple use new value
            result[key] = new_value
        end
    end
    return make_mergeable(result)
end

-- Makes it possible to merge this table with another one via "+" operator.
function make_mergeable(t)
    local metatable = {
        __add = merged
    }
    setmetatable(t, metatable)
    return t
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
    return {
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
end

function fire_after_timeout(millis)
    return {
        glue = {
            fire_mode = {
                kind = "AfterTimeout",
                timeout = millis,
            },
        },
    }
end

function fire_on_single_press()
    return {
        glue = {
            fire_mode = {
                kind = "OnSinglePress",
            },
        },
    }
end

function fire_on_double_press()
    return {
        glue = {
            fire_mode = {
                kind = "OnDoublePress",
            },
        },
    }
end

function slot_mode_is(slot_mode_index)
    return {
        activation_condition = {
            kind = "Bank",
            parameter = 3,
            bank_index = slot_mode_index,
        },
    }
end

local shift = make_mergeable(shift_pressed(true))
local not_shift = make_mergeable(shift_pressed(false))
local long_press = make_mergeable(fire_after_timeout(1000))
local single_press = make_mergeable(fire_on_single_press())
local double_press = make_mergeable(fire_on_double_press())

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
        column_normal_condition = {},
        row_normal_condition = {},
        slot_normal_condition = slot_mode_is(0),
        slot_delete_condition = slot_mode_is(1),
        slot_quantize_condition = slot_mode_is(2),
        undo = button("undo"),
        redo = button("redo"),
        play = button("play"),
        rec = button("record"),
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
        stop_clip = shift + button("row1/play"),
        solo = shift + button("row2/play"),
        record_arm = shift + button("row3/play"),
        mute = shift + button("row4/play"),
        track_select = shift + button("row5/play"),
        column_normal_condition = not_shift,
        row_normal_condition = not_shift,
        slot_normal_condition = not_shift,
        slot_delete_condition = shift + long_press,
        slot_quantize_condition = shift + double_press,
        undo = shift + button("play"),
        redo = shift + button("record"),
        play = not_shift + button("play"),
        rec = not_shift + button("record"),
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
    device_specific[device].play + {
        name = "Play arrangement",
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
    device_specific[device].undo + {
        name = "Undo",
        target = {
            kind = "ClipMatrixAction",
            action = "Undo",
        },
    },
    device_specific[device].redo + {
        name = "Redo",
        target = {
            kind = "ClipMatrixAction",
            action = "Redo",
        },
    },
    device_specific[device].sends + {
        name = "Switch send",
        group = "knob-sends",
        feedback_enabled = false,
        glue = {
            absolute_mode = "IncrementalButton",
            wrap = true,
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = 6,
            },
        },
    }

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
    {
        index = 6,
        name = "Send",
        value_count = 2,
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
        activation_condition = {
            kind = "Bank",
            parameter = 4,
            bank_index = 0,
        },
    },
    {
        id = "row-stop",
        name = "Row stop",
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
    local column_stop_mapping = device_specific[device].column_normal_condition + {
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
            local mapping = device_specific[device].column_normal_condition + {
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
    local send_vol_mapping = {
        name = "Knob " .. human_col .. " sends",
        group = "knob-sends",
        source = {
            kind = "Virtual",
            character = "Multi",
            id = col,
        },
        target = {
            kind = "RouteVolume",
            route = {
                address = "Dynamic",
                track = {
                    address = "FromClipColumn",
                    column = column,
                    context = "Playback",
                },
                expression = "p[6]",
            },
        },
    }
    table.insert(mappings, send_vol_mapping)
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

-- For each row
for row = 0, row_count - 1 do
    local human_row = row + 1
    local prefix = "row" .. human_row .. "/"
    local mapping = device_specific[device].row_normal_condition + {
        name = "Row " .. human_row .. " play",
        group = "row-play",
        feedback_enabled = false,
        source = {
            kind = "Virtual",
            character = "Button",
            id = prefix .. "play",
        },
        target = {
            kind = "ClipRowAction",
            row = {
                address = "Dynamic",
                expression = "p[1] + " .. row,
            },
            action = "Play",
        },
    }
    table.insert(mappings, mapping)
end

-- For each slot
for col = 0, column_count - 1 do
    local human_col = col + 1
    for row = 0, row_count - 1 do
        local human_row = row + 1
        local prefix = "col" .. human_col .. "/row" .. human_row .. "/"
        local slot_column_expression = "p[0] + " .. col
        local slot_row_expression = "p[1] + " .. row
        local slot_play = device_specific[device].slot_normal_condition + {
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
        local slot_clear = device_specific[device].slot_delete_condition + {
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
        local slot_quantize = device_specific[device].slot_quantize_condition + {
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