--- name: Launchpad Pro - Playtime
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- description: |
---     This main preset turns the Launchpad Pro into a capable device for controlling Playtime.
---     
---     The following features are not yet supported: Note, Device, User, Volume, Pan, Sends, Record,
---     Record Quantise.
--- setup_instructions: |
---     At the moment, you need to put the device manually into Live mode by pressing Setup + the top-left pad.
--- used_schemes: [novation/launchpad-pro/live]
--- required_features: [playtime]

--!strict

-- TODO-high Make record and play buttons control Playtime's sequencer

-- Configuration

local stop_column_if_slot_empty = true

-- Requires

local partial = require("partial")
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
        stop_clip = {
            index = 5,
            label = "Stop clip",
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

function clip_transport_action(col: number, row: number, action: realearn.PlaytimeSlotTransportAction)
    return partial_mapping {
        target = realearn.Target.PlaytimeSlotTransportAction {
            slot = create_slot_selector(col, row),
            action = action,
            stop_column_if_slot_empty = stop_column_if_slot_empty,
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

--- Sets the source interval for the purpose of displaying different LED colors for off/on states.
---
--- Both `off` and `on` should be integers between 0 and 127, representing the Launchpad colors.
---
--- In feedback direction, source min is sent when the target is off, source max is sent when it's on.
--- For Launchpad buttons, those values influence the LED color. 
---
--- Attention: Since the source interval also influences the control direction, this should in most cases
--- be used only for feedback-only mappings.
function colors(off: number, on: number)
    if off > on then
        error("on color must be >= off color")
    end
    return partial_mapping {
        glue = realearn.Glue {
            source_interval = { off / 127, on / 127 }
        },
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

-- Mappings

local normal_colors = colors(23, 122)
local shift_colors = colors(1, 3)
local arm_colors = colors(7, 72)
local select_colors = colors(39, 78)
local mute_colors = colors(15, 74)
local solo_colors = colors(47, 66)
local stop_clip_colors = colors(11, 84)

local mappings = {
    name("Shift") + button("shift") + shift_colors + set_param(params.shift.index),
    name("Click") + button("click") + normal_colors + toggle() + clip_matrix_action("ClickOnOffState"),
    name("Undo") + button("undo") + feedback_disabled() + shift_is(false) + clip_matrix_action("Undo"),
    name("Undo LED") + button("undo") + shift_is(false) + normal_colors + led_on_off(),
    name("Redo") + button("undo") + feedback_disabled() + shift_is(true) + clip_matrix_action("Redo"),
    name("Redo LED") + button("undo") + shift_is(true) + shift_colors + led_on_off(),
    name("Delete") + button("delete") + normal_colors + set_slot_mode(slot_modes.delete),
    name("Quantise") + button("quantize") + shift_is(false) + normal_colors + set_slot_mode(slot_modes.quantise),
    name("Record Quantise") + button("quantize") + shift_is(true) + shift_colors + toggle() + clip_matrix_action("MidiAutoQuantizationOnOffState"),
    name("Duplicate") + button("duplicate") + normal_colors + set_slot_mode(slot_modes.duplicate),
    name("Double") + button("double") + normal_colors + set_slot_mode(slot_modes.double),
}

function add_scroll_mappings(b: string, target: partial.Partial)
    table.insert(
        mappings,
        name(b) + button(b) + feedback_disabled() + turbo() + target
    )
    table.insert(
        mappings,
        name(b) + button(b) + control_disabled() + colors(0, 23) + target
    )
end

add_scroll_mappings("cursor-up", scroll_vertically(-1))
add_scroll_mappings("cursor-down", scroll_vertically(1))
add_scroll_mappings("cursor-left", scroll_horizontally(-1))
add_scroll_mappings("cursor-right", scroll_horizontally(1))

function add_column_mode_mappings(n: string, b: string, m: Mode, partial: partial.Partial)
    table.insert(mappings, name(n) + button(b) + partial + set_column_mode(m))
    table.insert(
        mappings,
        name(`{n} auto-release`)
            + feedback_disabled()
            + button(b)
            + auto_release()
            + compartment_parameter_target(params.column_mode.index)
    )
end


add_column_mode_mappings("Record Arm", "record-arm", column_modes.record_arm, arm_colors)
add_column_mode_mappings("Track Select", "track-select", column_modes.track_select, select_colors)
add_column_mode_mappings("Mute", "mute", column_modes.mute, mute_colors)
add_column_mode_mappings("Solo", "solo", column_modes.solo, solo_colors)
add_column_mode_mappings("Stop clip", "stop-clip", column_modes.stop_clip, stop_clip_colors)

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
            name("Trigger slot")
                + create_normal_control_condition(slot_modes.normal)
                + feedback_disabled()
                + slot_button(col, row)
                + clip_transport_action(col, row, "Trigger")
        )
        -- Control (delete)
        table.insert(
            mappings,
            name("Clear slot")
                + create_normal_control_condition(slot_modes.delete)
                + feedback_disabled()
                + slot_button(col, row)
                + clip_management_action(col, row, "ClearSlot")
        )
        -- Control (quantize)
        table.insert(
            mappings,
            name("Quantize slot")
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
                + create_normal_feedback_condition()
                + control_disabled()
                + slot_button(col, row)
                + playtime_util.slot_state_text_feedback()
                + clip_transport_action(col, row, "Trigger")
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
            + arm_colors
            + clip_column_action(col, "ArmStateExclusive")
    )
    -- Select track
    table.insert(
        mappings,
        name("Track Select")
            + column_mode_is(column_modes.track_select)
            + slot_button(col, row_count - 1)
            + toggle()
            + select_colors
            + column_track_target(col, "TrackSelectionState", true)
    )
    -- Mute track
    table.insert(
        mappings,
        name("Mute")
            + column_mode_is(column_modes.mute)
            + slot_button(col, row_count - 1)
            + toggle()
            + mute_colors
            + column_track_target(col, "TrackMuteState")
    )
    -- Solo track
    table.insert(
        mappings,
        name("Solo")
            + column_mode_is(column_modes.solo)
            + slot_button(col, row_count - 1)
            + toggle()
            + solo_colors
            + column_track_target(col, "TrackSoloState")
    )
    -- Stop column
    table.insert(
        mappings,
        name("Stop column")
            + column_mode_is(column_modes.stop_clip)
            + slot_button(col, row_count - 1)
            + stop_clip_colors
            + clip_column_action(col, "Stop")
    )
end

return realearn.Compartment {
    parameters = util.sorted_by_index(params),
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
