--- name: Launchpad Pro - Playtime
--- author: helgoboss
--- realearn_version: 2.16.0-pre.8
--- used_schemes: [novation/launchpad-pro/live]
--- required_features: [playtime]

--!strict

-- TODO-high Make record and play buttons control Playtime's sequencer

-- Configuration

-- Requires

local util = require("util")
local realearn = require("realearn")
local playtime_util = require("playtime_util")

-- Aliases

local partial_mapping = util.partial_mapping
local turbo = playtime_util.turbo
local scroll_vertically = playtime_util.scroll_vertically
local scroll_horizontally = playtime_util.scroll_horizontally

-- Types

type Mode = {
    index: number,
    label: string,
}

type ModeMap = { [string]: Mode }

type ParameterMap = { [string]: realearn.Parameter }

type GroupMap = { [string]: realearn.Group }

-- Constants

local column_count = 8
local row_count = 8
local slot_mode_count = 100
local column_mode_count = 100

-- Slot modes

local slot_modes: ModeMap = {
    normal = {
        index = 0,
        label = "Normal",
    },
    delete = {
        index = 1,
        label = "Delete",
    },
    quantise = {
        index = 2,
        label = "Quantise",
    },
    duplicate = {
        index = 3,
        label = "Duplicate",
    },
    double = {
        index = 4,
        label = "Double",
    },
}
local sorted_slot_modes = util.sorted_by_index(slot_modes)

-- Column modes

-- Using a function has the advantage that the resulting table is sealed and rejects property accesses with different properties
local function create_column_modes()
    return {
        normal = {
            index = 0,
            label = "Normal",
        },
        record_arm = {
            index = 1,
            label = "Record Arm",
        },
        track_select = {
            index = 2,
            label = "Track Select",
        },
        mute = {
            index = 3,
            label = "Mute",
        },
        solo = {
            index = 4,
            label = "Solo",
        },
    }
end
local column_modes = create_column_modes()
local sorted_column_modes = util.sorted_by_index(column_modes)

-- Parameters

local params: ParameterMap = {
    shift = {
        index = 0,
        name = "Shift modifier",
    },
    slot_mode = {
        index = 1,
        name = "Slot mode",
        value_count = slot_mode_count,
        value_labels = util.extract_labels(sorted_slot_modes),
    },
    column_mode = {
        index = 2,
        name = "Column mode",
        value_count = column_mode_count,
        value_labels = util.extract_labels(sorted_column_modes),
    },
}

-- Domain functions

function create_index_expression(variable_name: string, index: number): string
    return `{variable_name} + {index}`
end

function create_col_expression(col: number): string
    return create_index_expression("control_unit_column_index", col)
end

function create_row_expression(row: number): string
    return create_index_expression("control_unit_row_index", row)
end

function create_slot_selector(col: number, row: number): realearn.PlaytimeSlotDescriptor
    return realearn.PlaytimeSlotDescriptor.Dynamic {
        column_expression = create_col_expression(col),
        row_expression = create_row_expression(row),
    }
end

function create_column_selector(col: number): realearn.PlaytimeColumnDescriptor
    return realearn.PlaytimeColumnDescriptor.Dynamic {
        expression = create_col_expression(col),
    }
end

function create_row_selector(row: number): realearn.PlaytimeRowDescriptor
    return realearn.PlaytimeRowDescriptor.Dynamic {
        expression = create_row_expression(row),
    }
end

function multi(id: string)
    return partial_mapping {
        source = realearn.Source.Virtual {
            character = "Multi",
            id = id,
        },
    }
end

function button(id: string)
    return partial_mapping {
        source = realearn.Source.Virtual {
            character = "Button",
            id = id,
        },
    }
end

function row_play_button(row: number)
    return button(`row{row + 1}/play`)
end

function slot_button(col: number, row: number)
    return multi(`col{col + 1}/row{row + 1}/pad`)
end

function clip_column_action(col: number, action: realearn.PlaytimeColumnAction)
    return partial_mapping {
        target = realearn.Target.PlaytimeColumnAction {
            column = create_column_selector(col),
            action = action,
        },
    }
end

function clip_row_action(row: number, action: realearn.PlaytimeRowAction)
    return partial_mapping {
        target = realearn.Target.PlaytimeRowAction {
            row = create_row_selector(row),
            action = action,
        },
    }
end

function clip_column_track(col: number): realearn.TrackDescriptor
    return realearn.TrackDescriptor.FromClipColumn {
        column = create_column_selector(col),
        context = "Playback",
    }
end

function column_track_target(col: number, track_target_kind: realearn.TargetKind, exclusive: boolean?)
    return partial_mapping {
        target = {
            kind = track_target_kind,
            track = clip_column_track(col),
            exclusivity = if exclusive then "WithinFolderOnOnly" else nil,
        } :: any,
    }
end

function clip_transport_action(
    col: number,
    row: number,
    action: realearn.PlaytimeSlotTransportAction,
    record_only_if_track_armed: boolean
)
    return partial_mapping {
        target = realearn.Target.PlaytimeSlotTransportAction {
            slot = create_slot_selector(col, row),
            action = action,
            record_only_if_track_armed = record_only_if_track_armed,
            stop_column_if_slot_empty = true,
        },
    }
end

function clip_management_action(col: number, row: number, action: realearn.PlaytimeSlotManagementActionKind)
    return partial_mapping {
        glue = {
            button_filter = "PressOnly",
        },
        target = realearn.Target.PlaytimeSlotManagementAction {
            slot = create_slot_selector(col, row),
            action = {
                kind = action,
            } :: any,
        },
    }
end

function clip_matrix_action(action: realearn.PlaytimeMatrixAction)
    return partial_mapping {
        target = realearn.Target.PlaytimeMatrixAction {
            action = action,
        },
    }
end

function toggle()
    return partial_mapping {
        glue = {
            absolute_mode = "ToggleButton",
        },
    }
end

function control_disabled()
    return partial_mapping {
        control_enabled = false,
        visible_in_projection = false,
    }
end

function feedback_disabled()
    return partial_mapping {
        feedback_enabled = false,
    }
end

function set_param(index: number)
    return partial_mapping {
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = index,
            },
        },
    }
end

function name(n: string)
    return partial_mapping {
        name = n,
    }
end

function colors(off: number, on: number)
    return partial_mapping {
        glue = realearn.Glue {
            source_interval = { off, on },
        },
    }
end

function group(g: realearn.Group)
    return partial_mapping {
        group = g.id,
    }
end

function set_mode_momentary(mode: Mode, mode_count: number, mode_param_index: number)
    local target_value = mode.index / (mode_count - 1)
    return partial_mapping {
        name = mode.label,
        glue = realearn.Glue {
            target_value_sequence = `0, {target_value}`,
            feedback = realearn.Feedback.Numeric {
                transformation = `x = y == {target_value}`,
            },
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = mode_param_index,
            },
        },
    }
end

function compartment_parameter_target(index: number)
    return partial_mapping {
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = index,
            },
        },
    }
end

function set_mode_toggle(mode: Mode, mode_count: number, mode_param_index: number)
    local target_value = mode.index / (mode_count - 1)
    return partial_mapping {
        name = mode.label,
        glue = realearn.Glue {
            button_filter = "PressOnly",
            control_transformation = `y = y == {target_value} ? 0 : {target_value}`,
            feedback = realearn.Feedback.Numeric {
                transformation = `x = y == {target_value}`,
            },
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = mode_param_index,
            },
        },
    }
end

--- Sets the value back to zero if the button is pressed longer and then released.
---
--- Can be used to turn toggle buttons into momentary buttons when pressed longer.
function auto_release()
    return partial_mapping {
        glue = realearn.Glue {
            target_interval = { 0.0, 0.0 },
            fire_mode = realearn.FireMode.Normal {
                press_duration_interval = { 300, 10000 },
            },
        },
    }
end

function set_slot_mode(mode: Mode)
    return set_mode_momentary(mode, slot_mode_count, params.slot_mode.index)
end

function set_column_mode(mode: Mode)
    return set_mode_toggle(mode, column_mode_count, params.column_mode.index)
end

function slot_mode_is(slot_mode: Mode)
    return partial_mapping {
        activation_condition = realearn.ActivationCondition.Bank {
            parameter = params.slot_mode.index,
            bank_index = slot_mode.index,
        },
    }
end

function column_mode_is(column_mode: Mode)
    return partial_mapping {
        activation_condition = realearn.ActivationCondition.Bank {
            parameter = params.column_mode.index,
            bank_index = column_mode.index,
        },
    }
end

function column_mode_is_normal_and_slot_mode_is(slot_mode: Mode)
    return partial_mapping {
        activation_condition = realearn.ActivationCondition.Expression {
            condition = `p[{params.column_mode.index}] == {column_modes.normal.index} && p[{params.slot_mode.index}] == {slot_mode.index}`,
        },
    }
end

function shift_is(on: boolean)
    return partial_mapping {
        activation_condition = realearn.ActivationCondition.Modifier {
            modifiers = {
                {
                    parameter = params.shift.index,
                    on = on,
                },
            },
        },
    }
end

function led_on_off()
    return partial_mapping {
        target = realearn.Target.Dummy {},
    }
end

-- Groups

local groups: GroupMap = {
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
}
util.set_keys_as_ids(groups)

-- Mappings

local mappings = {
    name("Shift") + button("shift") + colors(0.12, 0.98) + set_param(params.shift.index),
    name("Click") + button("click") + colors(0.12, 0.98) + toggle() + clip_matrix_action("ClickOnOffState"),
    name("Undo") + button("undo") + feedback_disabled() + shift_is(false) + clip_matrix_action("Undo"),
    name("Undo LED") + button("undo") + shift_is(false) + colors(0.12, 0.98) + led_on_off(),
    name("Redo") + button("undo") + feedback_disabled() + shift_is(true) + clip_matrix_action("Redo"),
    name("Redo LED") + button("undo") + shift_is(true) + colors(0.23, 0.70) + led_on_off(),
    name("Scroll up") + button("cursor-up") + turbo() + scroll_vertically(-1),
    name("Scroll down") + button("cursor-down") + turbo() + scroll_vertically(1),
    name("Scroll left") + button("cursor-left") + turbo() + scroll_horizontally(-1),
    name("Scroll right") + button("cursor-right") + turbo() + scroll_horizontally(1),
    name("Delete") + button("delete") + colors(0.12, 0.98) + set_slot_mode(slot_modes.delete),
    name("Quantise") + button("quantize") + colors(0.12, 0.98) + set_slot_mode(slot_modes.quantise),
    name("Duplicate") + button("duplicate") + colors(0.12, 0.98) + set_slot_mode(slot_modes.duplicate),
    name("Double") + button("double") + colors(0.12, 0.98) + set_slot_mode(slot_modes.double),
}

function add_column_mode_mappings(n: string, b: string, m: Mode)
    table.insert(mappings, name(n) + button(b) + colors(0.12, 0.98) + set_column_mode(m))
    table.insert(
        mappings,
        name(`{n} auto-release`)
            + feedback_disabled()
            + button(b)
            + auto_release()
            + compartment_parameter_target(params.column_mode.index)
    )
end

add_column_mode_mappings("Record Arm", "record-arm", column_modes.record_arm)
add_column_mode_mappings("Track Select", "track-select", column_modes.track_select)
add_column_mode_mappings("Mute", "mute", column_modes.mute)
add_column_mode_mappings("Solo", "solo", column_modes.solo)

-- For each row
for row = 0, row_count - 1 do
    table.insert(
        mappings,
        name("Play scene") + feedback_disabled() + row_play_button(row) + clip_row_action(row, "PlayScene")
    )
    table.insert(mappings, name("Play scene LED") + row_play_button(row) + led_on_off())
end

-- For each slot
for col = 0, column_count - 1 do
    for row = 0, row_count - 1 do
        local function create_normal_control_condition(slot_mode: Mode)
            if row == row_count - 1 then
                -- The last row is special. If the column mode is not the normal one, it will carry out different functions.
                return column_mode_is_normal_and_slot_mode_is(slot_mode)
            end
            return slot_mode_is(slot_mode)
        end
        local function create_normal_feedback_condition()
            if row == row_count - 1 then
                -- The last row is special. If the column mode is not the normal one, it will carry out different functions.
                return column_mode_is(column_modes.normal)
            end
            return partial_mapping {}
        end
        -- Control (normal)
        table.insert(
            mappings,
            name("Rec/play/stop")
                + group(groups.slot_play)
                + create_normal_control_condition(slot_modes.normal)
                + feedback_disabled()
                + slot_button(col, row)
                + toggle()
                + clip_transport_action(col, row, "RecordPlayStop", true)
        )
        -- Control (delete)
        table.insert(
            mappings,
            name("Clear slot")
                + group(groups.slot_clear)
                + create_normal_control_condition(slot_modes.delete)
                + feedback_disabled()
                + slot_button(col, row)
                + clip_management_action(col, row, "ClearSlot")
        )
        -- Control (quantize)
        table.insert(
            mappings,
            name("Quantize slot")
                + group(groups.slot_quantize)
                + create_normal_control_condition(slot_modes.quantise)
                + feedback_disabled()
                + slot_button(col, row)
                + clip_management_action(col, row, "QuantizationOnOffState")
        )
        -- Control (duplicate)
        table.insert(
            mappings,
            name("Duplicate slot")
                + create_normal_control_condition(slot_modes.duplicate)
                + feedback_disabled()
                + slot_button(col, row)
                + clip_management_action(col, row, "Duplicate")
        )
        -- Control (double)
        table.insert(
            mappings,
            name("Double slot")
                + create_normal_control_condition(slot_modes.double)
                + feedback_disabled()
                + slot_button(col, row)
                + clip_management_action(col, row, "Double")
        )
        -- Feedback
        table.insert(
            mappings,
            name("Slot feedback")
                + group(groups.slot_feedback)
                + create_normal_feedback_condition()
                + control_disabled()
                + slot_button(col, row)
                + playtime_util.slot_state_text_feedback()
                + clip_transport_action(col, row, "RecordPlayStop", true)
        )
    end
end

-- For the last row of slots
for col = 0, column_count - 1 do
    -- Record-arm track
    table.insert(
        mappings,
        name("Record Arm")
            + column_mode_is(column_modes.record_arm)
            + slot_button(col, row_count - 1)
            + toggle()
            + column_track_target(col, "TrackArmState", true)
    )
    -- Select track
    table.insert(
        mappings,
        name("Track Select")
            + column_mode_is(column_modes.track_select)
            + slot_button(col, row_count - 1)
            + toggle()
            + column_track_target(col, "TrackSelectionState", true)
    )
    -- Mute track
    table.insert(
        mappings,
        name("Mute")
            + column_mode_is(column_modes.mute)
            + slot_button(col, row_count - 1)
            + toggle()
            + column_track_target(col, "TrackMuteState")
    )
    -- Solo track
    table.insert(
        mappings,
        name("Solo")
            + column_mode_is(column_modes.solo)
            + slot_button(col, row_count - 1)
            + toggle()
            + column_track_target(col, "TrackSoloState")
    )
end


return realearn.Compartment {
    parameters = util.sorted_by_index(params),
    groups = util.to_array(groups),
    mappings = mappings :: any,
    custom_data = {
        playtime = {
            control_unit = {
                column_count = column_count,
                row_count = row_count,
            },
        },
    },
}
