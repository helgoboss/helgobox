--- name: APC Key 25
--- realearn_version: 2.16.0-pre.8

-- Configuration

local use_column_stop_buttons = true

-- Utility functions

--- Takes a key-value table and adds a new attribute `id` to each value that
--- corresponds to the key.
function set_keys_as_ids(t)
    for key, value in pairs(t) do
        value.id = key
    end
end

--- Puts each `label` property value of the given array into a new array.
function extract_labels(array)
    local labels = {}
    for _, element in ipairs(array) do
        table.insert(labels, element.label)
    end
    return labels
end

--- Converts the given key-value table to an array table.
function to_array(t)
    local array = {}
    for _, v in pairs(t) do
        table.insert(array, v)
    end
    return array
end

--- Returns a new table that's the given table turned into an array
--- and sorted by the `index` key.
function sorted_by_index(t)
    local sorted = to_array(t)
    local compare_index = function(left, right)
        return left.index < right.index
    end
    table.sort(sorted, compare_index)
    return sorted
end

--- Clones a table.
function clone(t)
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

--- Makes it possible to merge this table with another one via "+" operator.
function make_mergeable(t)
    local metatable = {
        __add = merged
    }
    setmetatable(t, metatable)
    return t
end

function PartialMapping(t)
    return make_mergeable(t)
end

-- Constants

local column_count = 8
local row_count = 5
local column_mode_count = 100
local knob_mode_count = 100

-- Column modes

local column_modes = {
    stop = {
        index = 0,
        label = "Stop clip",
    },
    solo = {
        index = 1,
        label = "Solo",
    },
    record_arm = {
        index = 2,
        label = "Record arm",
    },
    mute = {
        index = 3,
        label = "Mute",
    },
    select = {
        index = 4,
        label = "Track select",
    },
}
local sorted_column_modes = sorted_by_index(column_modes)

-- Knob modes
local knob_modes = {
    volume = {
        index = 0,
        label = "Volume",
    },
    pan = {
        index = 1,
        label = "Pan",
    },
    sends = {
        index = 2,
        label = "Sends",
    },
    device = {
        index = 3,
        label = "Device",
    },
}
local sorted_knob_modes = sorted_by_index(knob_modes)

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
    shift = {
        index = 2,
        name = "Shift modifier",
    },
    column_mode = {
        index = 3,
        name = "Column mode",
        value_count = column_mode_count,
        value_labels = extract_labels(sorted_column_modes),
    },
    knob_mode = {
        index = 4,
        name = "Knob mode",
        value_count = knob_mode_count,
        value_labels = extract_labels(sorted_knob_modes),
    },
    send = {
        index = 6,
        name = "Send",
        value_count = 2,
    },
    sustain = {
        index = 7,
        name = "Sustain modifier",
    },
}


-- Domain functions

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

function create_column_selector(col)
    return {
        address = "Dynamic",
        expression = create_col_expression(col),
    }
end

function create_row_selector(row)
    return {
        address = "Dynamic",
        expression = create_row_expression(row),
    }
end

function multi(index)
    return PartialMapping {
        source = {
            kind = "Virtual",
            character = "Multi",
            id = index,
        },
    }
end

function button(id)
    return PartialMapping {
        source = {
            kind = "Virtual",
            id = id,
            character = "Button",
        },
    }
end

function column_stop_button(col)
    return button("col" .. (col + 1) .. "/stop")
end

function row_play_button(row)
    return button("row" .. (row + 1) .. "/play")
end

function slot_button(col, row)
    return button("col" .. (col + 1) .. "/row" .. (row + 1) .. "/pad")
end

function shift_pressed(on)
    return PartialMapping {
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = params.shift.index,
                    on = on,
                },
                {
                    parameter = params.sustain.index,
                    on = false,
                },
            },
        },
    }
end

function sustain_pressed(on)
    return PartialMapping {
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = params.sustain.index,
                    on = on,
                },
                {
                    parameter = params.shift.index,
                    on = false,
                },
            },
        },
    }
end

function shift_or_sustain_pressed()
    return PartialMapping {
        activation_condition = {
            kind = "Expression",
            condition = "p[2] || p[7]",
        },
    }
end

function shift_and_sustain_pressed()
    return PartialMapping {
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = params.sustain.index,
                    on = true,
                },
                {
                    parameter = params.shift.index,
                    on = true,
                },
            },
        },
    }
end

function turbo()
    return PartialMapping {
        glue = {
            fire_mode = {
                kind = "AfterTimeoutKeepFiring",
                timeout = 0,
                rate = 100,
            },
        },
    }
end

function fire_max(millis)
    return PartialMapping {
        glue = {
            fire_mode = {
                kind = "Normal",
                press_duration_interval = { 0, millis }
            },
        },
    }
end

function fire_after_timeout(millis)
    return PartialMapping {
        glue = {
            fire_mode = {
                kind = "AfterTimeout",
                timeout = millis,
            },
        },
    }
end

function fire(kind, max_duration)
    return {
        glue = {
            fire_mode = {
                kind = kind,
                max_duration = max_duration
            },
        },
    }
end

local no_mod = shift_pressed(false)
local shift = shift_pressed(true)
local sustain = sustain_pressed(true)
local shift_or_sustain = shift_or_sustain_pressed()
local shift_and_sustain = shift_and_sustain_pressed()
local short_press = fire_max(200)
local long_press = fire_after_timeout(1000)
local single_press = fire("OnSinglePress", 200)
local double_press = fire("OnDoublePress")

function clip_matrix_action(action)
    return PartialMapping {
        target = {
            kind = "ClipMatrixAction",
            action = action,
        },
    }
end

function clip_column_action(col, action)
    return PartialMapping {
        target = {
            kind = "ClipColumnAction",
            column = create_column_selector(col),
            action = action,
        },
    }
end

function clip_row_action(row, action)
    return PartialMapping {
        target = {
            kind = "ClipRowAction",
            row = create_row_selector(row),
            action = action,
        },
    }
end

function clip_column_track(col)
    return {
        address = "FromClipColumn",
        column = create_column_selector(col),
        context = "Playback",
    }
end

function column_track_target(col, track_target_kind, exclusive)
    return PartialMapping {
        target = {
            kind = track_target_kind,
            track = clip_column_track(col),
            exclusivity = exclusive and "WithinFolderOnOnly" or nil,
        },
    }
end

function route_target(col, route_target_kind)
    return PartialMapping {
        target = {
            kind = route_target_kind,
            route = {
                address = "Dynamic",
                track = clip_column_track(col),
                expression = "p[" .. params.send.index .. "]",
            },
        },
    }
end

function clip_transport_action(col, row, action, record_only_if_track_armed)
    return PartialMapping {
        target = {
            kind = "ClipTransportAction",
            slot = create_slot_selector(col, row),
            action = action,
            record_only_if_track_armed = record_only_if_track_armed,
            stop_column_if_slot_empty = true,
        },
    }
end

function clip_management_action(col, row, action)
    return PartialMapping {
        target = {
            kind = "ClipManagement",
            slot = create_slot_selector(col, row),
            action = {
                kind = action,
            },
        },
    }
end

function adjust_clip_section_length_action(col, row, factor)
    return PartialMapping {
        target = {
            kind = "ClipManagement",
            slot = create_slot_selector(col, row),
            action = {
                kind = "AdjustClipSectionLength",
                factor = factor,
            },
        },
    }
end

function slot_state_text_feedback()
    return PartialMapping {
        glue = {
            feedback = {
                kind = "Text",
                text_expression = "{{ target.slot_state.id }}",
            },
        },
    }
end

function transport_action(action)
    return PartialMapping {
        target = {
            kind = "TransportAction",
            action = action,
        },
    }
end

function reaper_action(command_id)
    return PartialMapping {
        target = {
            kind = "ReaperAction",
            command = command_id,
            invocation = "Trigger",
        },
    }
end

function toggle()
    return PartialMapping {
        glue = {
            absolute_mode = "ToggleButton",
        },
    }
end

function incremental()
    return PartialMapping {
        glue = {
            absolute_mode = "IncrementalButton",
        },
    }
end

function wrap()
    return PartialMapping {
        glue = {
            wrap = true,
        },
    }
end

function control_disabled()
    return PartialMapping {
        control_enabled = false,
        visible_in_projection = false,
    }
end

function feedback_disabled()
    return PartialMapping {
        feedback_enabled = false,
    }
end

function scroll_horizontally(amount)
    return scroll(params.column_offset.index, amount)
end

function scroll_vertically(amount)
    return scroll(params.row_offset.index, amount)
end

function scroll(param_index, amount)
    local abs_amount = math.abs(amount)
    return {
        glue = {
            absolute_mode = "IncrementalButton",
            step_factor_interval = { abs_amount, abs_amount },
            reverse = amount < 0,
            feedback = {
                kind = "Numeric",
                transformation = "x = 1",
            },
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

function set_param(index)
    return PartialMapping {
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = index,
            },
        },
    }
end

function name(name)
    return PartialMapping {
        name = name,
    }
end

function group(g)
    return PartialMapping {
        group = g.id,
    }
end

function set_mode(mode, mode_count, mode_param_index)
    local target_value = mode.index / (mode_count - 1)
    return PartialMapping {
        name = mode.label,
        glue = {
            target_interval = { target_value, target_value },
            out_of_range_behavior = "Min",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                index = mode_param_index,
            },
        },
    }
end

function set_column_mode(mode)
    return set_mode(mode, column_mode_count, params.column_mode.index)
end

function set_knob_mode(mode)
    return set_mode(mode, knob_mode_count, params.knob_mode.index)
end

function column_mode_is(column_mode)
    return {
        kind = "Bank",
        parameter = params.column_mode.index,
        bank_index = column_mode.index,
    }
end

function knob_mode_is(knob_mode)
    return {
        kind = "Bank",
        parameter = params.knob_mode.index,
        bank_index = knob_mode.index,
    }
end

-- Groups

local groups = {
    slot_modes = {
        name = "Slot modes",
    },
    column_modes = {
        name = "Column modes",
    },
    record_settings = {
        name = "Record settings",
    },
    knob_modes = {
        name = "Knob modes",
    },
    slot_feedback = {
        name = "Slot feedback",
    },
    slot_play = {
        name = "Slot play",
    },
    slot_clear = {
        name = "Slot clear",
    },
    slot_quantize = {
        name = "Slot quantize",
    },
    slot_copy_or_paste = {
        name = "Slot copy or paste",
    },
    slot_double = {
        name = "Slot double section",
    },
    slot_halve = {
        name = "Slot halve section",
    },
    column_stop = {
        name = "Column stop",
        activation_condition = column_mode_is(column_modes.stop),
    },
    row_play_scene = {
        name = "Row play scene",
    },
    row_build_scene = {
        name = "Row build scene",
    },
    row_copy_or_paste_scene = {
        name = "Row copy or paste scene",
    },
    row_clear_scene = {
        name = "Row clear scene",
    },
    column_solo = {
        name = "Column solo",
        activation_condition = column_mode_is(column_modes.solo),
    },
    column_record_arm = {
        name = "Column record arm",
        activation_condition = column_mode_is(column_modes.record_arm),
    },
    column_mute = {
        name = "Column mute",
        activation_condition = column_mode_is(column_modes.mute),
    },
    column_select = {
        name = "Column select",
        activation_condition = column_mode_is(column_modes.select),
    },
    knob_volume = {
        name = "Knob volume",
        activation_condition = knob_mode_is(knob_modes.volume),
    },
    knob_pan = {
        name = "Knob pan",
        activation_condition = knob_mode_is(knob_modes.pan),
    },
    knob_sends = {
        name = "Knob sends",
        activation_condition = knob_mode_is(knob_modes.sends),
    },
    knob_device = {
        name = "Knob device",
        activation_condition = knob_mode_is(knob_modes.device),
    },
}
set_keys_as_ids(groups)

-- Mappings

local mappings = {
    name("Stop all clips") + no_mod + button("stop-all-clips") + clip_matrix_action("Stop"),
    name("Play/stop") + no_mod + button("play") + toggle() + transport_action("PlayStop"),
    name("Shift modifier") + button("shift") + set_param(params.shift.index),
    name("Sustain modifier") + button("sustain") + set_param(params.sustain.index),
    name("Undo") + shift + button("play") + clip_matrix_action("Undo"),
    name("Redo") + shift + button("record") + clip_matrix_action("Redo"),
    name("Build scene") + sustain + button("record") + clip_matrix_action("BuildScene"),
    name("Click") + shift + button("stop-all-clips") + reaper_action(40364),
    name("Switch send") + group(groups.knob_sends) + feedback_disabled() + shift + button("col7/stop") + incremental() + wrap() + set_param(params.send.index),
    name("Column stop mode") + group(groups.column_modes) + shift + short_press + button("row1/play") + set_column_mode(column_modes.stop),
    name("Column solo mode") + group(groups.column_modes) + shift + short_press + button("row2/play") + set_column_mode(column_modes.solo),
    name("Column arm mode") + group(groups.column_modes) + shift + short_press + button("row3/play") + set_column_mode(column_modes.record_arm),
    name("Column mute mode") + group(groups.column_modes) + shift + short_press + button("row4/play") + set_column_mode(column_modes.mute),
    name("Column select mode") + group(groups.column_modes) + shift + short_press + button("row5/play") + set_column_mode(column_modes.select),
    name("Record 1 bar") + group(groups.record_settings) + shift_and_sustain + button("row1/play") + clip_matrix_action("SetRecordDurationToOneBar"),
    name("Record 2 bars") + group(groups.record_settings) + shift_and_sustain + button("row2/play") + clip_matrix_action("SetRecordDurationToTwoBars"),
    name("Record 4 bars") + group(groups.record_settings) + shift_and_sustain + button("row3/play") + clip_matrix_action("SetRecordDurationToFourBars"),
    name("Record 8 bars") + group(groups.record_settings) + shift_and_sustain + button("row4/play") + clip_matrix_action("SetRecordDurationToEightBars"),
    name("Record open-ended") + group(groups.record_settings) + shift_and_sustain + button("stop-all-clips") + clip_matrix_action("SetRecordDurationToOpenEnd"),
}

if use_column_stop_buttons then
    -- Scrolling
    table.insert(mappings, name("Scroll up") + shift_or_sustain + button("col1/stop") + feedback_disabled() + turbo() + scroll_vertically(-1))
    table.insert(mappings, name("Scroll down") + shift_or_sustain + button("col2/stop") + feedback_disabled() + turbo() + scroll_vertically(1))
    table.insert(mappings, name("Scroll left") + shift_or_sustain + button("col3/stop") + feedback_disabled() + turbo() + scroll_horizontally(-1))
    table.insert(mappings, name("Scroll right") + shift_or_sustain + button("col4/stop") + feedback_disabled() + turbo() + scroll_horizontally(1))
    -- Modes
    table.insert(mappings, name("Knob volume mode") + group(groups.knob_modes) + shift + button("col5/stop") + set_knob_mode(knob_modes.volume))
    table.insert(mappings, name("Knob pan mode") + group(groups.knob_modes) + shift + button("col6/stop") + set_knob_mode(knob_modes.pan))
    table.insert(mappings, name("Knob send mode") + group(groups.knob_modes) + shift + button("col7/stop") + set_knob_mode(knob_modes.sends))
    table.insert(mappings, name("Knob device mode") + group(groups.knob_modes) + shift + button("col8/stop") + set_knob_mode(knob_modes.device))
end

-- For each column
for col = 0, column_count - 1 do
    -- Column stop button functions
    if use_column_stop_buttons then
        table.insert(mappings, name("Stop column") + group(groups.column_stop) + no_mod + column_stop_button(col) + clip_column_action(col, "Stop"))
        table.insert(mappings, name("Solo track") + group(groups.column_solo) + no_mod + toggle() + column_stop_button(col) + column_track_target(col, "TrackSoloState"))
        table.insert(mappings, name("Arm track") + group(groups.column_record_arm) + no_mod + toggle() + column_stop_button(col) + column_track_target(col, "TrackArmState", false))
        table.insert(mappings, name("Mute track") + group(groups.column_mute) + no_mod + toggle() + column_stop_button(col) + column_track_target(col, "TrackMuteState"))
        table.insert(mappings, name("Select track") + group(groups.column_select) + no_mod + toggle() + column_stop_button(col) + column_track_target(col, "TrackSelectionState", true))
    end
    -- Knob functions
    table.insert(mappings, name("Track volume") + group(groups.knob_volume) + multi(col) + column_track_target(col, "TrackVolume"))
    table.insert(mappings, name("Track pan") + group(groups.knob_pan) + multi(col) + column_track_target(col, "TrackPan"))
    table.insert(mappings, name("Track send volume") + group(groups.knob_sends) + multi(col) + route_target(col, "RouteVolume"))
end

-- For each row
for row = 0, row_count - 1 do
    table.insert(mappings, name("Play scene") + group(groups.row_play_scene) + feedback_disabled() + no_mod + row_play_button(row) + clip_row_action(row, "PlayScene"))
    table.insert(mappings, name("Copy or paste") + group(groups.row_copy_or_paste_scene) + sustain + short_press + row_play_button(row) + clip_row_action(row, "CopyOrPasteScene"))
    table.insert(mappings, name("Long = Clear") + group(groups.row_clear_scene) + feedback_disabled() + sustain + long_press + row_play_button(row) + clip_row_action(row, "ClearScene"))
end

-- For each slot
for col = 0, column_count - 1 do
    for row = 0, row_count - 1 do
        -- Feedback
        table.insert(mappings, name("Slot feedback") + group(groups.slot_feedback) + control_disabled() + slot_button(col, row) + slot_state_text_feedback() + clip_transport_action(col, row, "RecordPlayStop", true))
        -- Control
        table.insert(mappings, name("Rec/play/stop") + group(groups.slot_play) + feedback_disabled() + no_mod + slot_button(col, row) + toggle() + clip_transport_action(col, row, "RecordPlayStop", true))
        table.insert(mappings, name("Copy or paste") + group(groups.slot_copy_or_paste) + feedback_disabled() + sustain + single_press + slot_button(col, row) + toggle() + clip_management_action(col, row, "CopyOrPasteClip"))
        table.insert(mappings, name("Long = Delete") + group(groups.slot_clear) + feedback_disabled() + sustain + long_press + slot_button(col, row) + clip_management_action(col, row, "ClearSlot"))
        table.insert(mappings, name("2x = Edit") + group(groups.slot_quantize) + feedback_disabled() + sustain + double_press + slot_button(col, row) + toggle() + clip_management_action(col, row, "EditClip"))
        --table.insert(mappings, name("Overdub clip") + group(groups.slot_play) + feedback_disabled() + shift + single_press + slot_button(col, row) + toggle() + clip_transport_action(col, row, "RecordStop", false))
        --table.insert(mappings, name("2x = Double section") + group(groups.slot_double) + feedback_disabled() + shift + double_press + slot_button(col, row) + adjust_clip_section_length_action(col, row, 2))
        --table.insert(mappings, name("1x = Halve section") + group(groups.slot_double) + feedback_disabled() + shift + single_press + slot_button(col, row) + adjust_clip_section_length_action(col, row, 0.5))
        table.insert(mappings, name("Fill slot") + group(groups.slot_quantize) + feedback_disabled() + shift + single_press + slot_button(col, row) + clip_management_action(col, row, "FillSlotWithSelectedItem"))
    end
end

return {
    kind = "MainCompartment",
    value = {
        parameters = sorted_by_index(params),
        groups = to_array(groups),
        mappings = mappings,
    },
}