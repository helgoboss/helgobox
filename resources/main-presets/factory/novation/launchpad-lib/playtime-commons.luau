--!strict

local partial = require("partial")
local util = require("util")
local realearn = require("realearn")
local playtime_util = require("playtime_util")

local module = {}

export type LaunchpadPlaytimePresetConfig = {
    pad_character: realearn.VirtualControlElementCharacter,
    stop_column_if_slot_empty: boolean,
    has_dedicated_column_action_buttons: boolean,
    has_shift_button: boolean,
    has_delete_button: boolean,
    has_quantize_button: boolean,
    has_fixed_length_button: boolean,
    has_duplicate_button: boolean,
    has_play_button: boolean,
    has_record_button: boolean,
    undo_button_id: string?,
    undo_needs_shift: boolean,
    redo_button_id: string?,
    redo_needs_shift: boolean,
    click_button_id: string?,
    click_needs_shift: boolean,
    double_button_id: string?,
    double_needs_shift: boolean,
    tap_button_id: string?,
    tap_needs_shift: boolean,
    use_last_row_play_button_for_column_modes: boolean,
}

function module.create_compartment(config: LaunchpadPlaytimePresetConfig): realearn.Compartment
    -- TODO-high-playtime-before-release Make record and play buttons control Playtime's sequencer

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
    local long_press_millis = 300

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
            set_record_length = {
                index = 6,
                label = "Set record length",
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

    local function create_index_expression(variable_name: string, index: number): string
        return `{variable_name} + {index}`
    end

    local function create_col_expression(col: number): string
        return create_index_expression("control_unit_column_index", col)
    end

    local function create_row_expression(row: number): string
        return create_index_expression("control_unit_row_index", row)
    end

    local function create_slot_selector(col: number, row: number): realearn.PlaytimeSlotDescriptor
        return realearn.PlaytimeSlotDescriptor.Dynamic {
            column_expression = create_col_expression(col),
            row_expression = create_row_expression(row),
        }
    end

    local function create_column_selector(col: number): realearn.PlaytimeColumnDescriptor
        return realearn.PlaytimeColumnDescriptor.Dynamic {
            expression = create_col_expression(col),
        }
    end

    local function create_row_selector(row: number): realearn.PlaytimeRowDescriptor
        return realearn.PlaytimeRowDescriptor.Dynamic {
            expression = create_row_expression(row),
        }
    end

    local function pad(id: string)
        return partial_mapping {
            source = realearn.Source.Virtual {
                character = config.pad_character,
                id = id,
            },
        }
    end

    local function button(id: string)
        return partial_mapping {
            source = realearn.Source.Virtual {
                character = "Button",
                id = id,
            },
        }
    end

    local function row_play_button(row: number)
        return button(`row{row + 1}/play`)
    end

    local function slot_button(col: number, row: number)
        return pad(`col{col + 1}/row{row + 1}/pad`)
    end

    local function column_action_button(col: number)
        return button(`col{col + 1}/action`)
    end

    local function clip_column_action(col: number, action: realearn.PlaytimeColumnAction)
        return partial_mapping {
            target = realearn.Target.PlaytimeColumnAction {
                column = create_column_selector(col),
                action = action,
            },
        }
    end

    local function clip_row_action(row: number, action: realearn.PlaytimeRowAction)
        return partial_mapping {
            target = realearn.Target.PlaytimeRowAction {
                row = create_row_selector(row),
                action = action,
            },
        }
    end

    local function clip_column_track(col: number): realearn.TrackDescriptor
        return realearn.TrackDescriptor.FromClipColumn {
            column = create_column_selector(col),
            context = "Playback",
        }
    end

    local function column_track_target(col: number, track_target_kind: realearn.TargetKind, exclusive: boolean?)
        return partial_mapping {
            target = {
                kind = track_target_kind,
                track = clip_column_track(col),
                exclusivity = if exclusive then "WithinFolderOnOnly" else nil,
            } :: any,
        }
    end

    local function clip_transport_action(col: number, row: number, action: realearn.PlaytimeSlotTransportAction)
        return partial_mapping {
            target = realearn.Target.PlaytimeSlotTransportAction {
                slot = create_slot_selector(col, row),
                action = action,
                stop_column_if_slot_empty = config.stop_column_if_slot_empty,
            },
        }
    end

    local function clip_management_action(col: number, row: number, action: realearn.PlaytimeSlotManagementAction)
        return partial_mapping {
            glue = {
                button_filter = "PressOnly",
            },
            target = realearn.Target.PlaytimeSlotManagementAction {
                slot = create_slot_selector(col, row),
                action = action,
            },
        }
    end

    local function clip_matrix_action(action: realearn.PlaytimeMatrixAction)
        return partial_mapping {
            target = realearn.Target.PlaytimeMatrixAction {
                action = action,
            },
        }
    end

    local function toggle()
        return partial_mapping {
            glue = {
                absolute_mode = "ToggleButton",
            },
        }
    end

    local function control_disabled()
        return partial_mapping {
            control_enabled = false,
            visible_in_projection = false,
        }
    end

    local function feedback_disabled()
        return partial_mapping {
            feedback_enabled = false,
        }
    end

    local function set_param(index: number)
        return partial_mapping {
            target = realearn.Target.CompartmentParameterValue {
                parameter = realearn.CompartmentParameterDescriptor.ById {
                    index = index,
                },
            },
        }
    end

    local function name(n: string)
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
    local function colors(off: number, on: number)
        if off > on then
            error("on color must be >= off color")
        end
        return partial_mapping {
            glue = realearn.Glue {
                source_interval = { off / 127, on / 127 },
            },
        }
    end

    local function set_mode_momentary(mode: Mode, mode_count: number, mode_param_index: number)
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

    local function compartment_parameter_target(index: number)
        return partial_mapping {
            target = realearn.Target.CompartmentParameterValue {
                parameter = realearn.CompartmentParameterDescriptor.ById {
                    index = index,
                },
            },
        }
    end

    local function set_mode_toggle(mode: Mode, mode_count: number, mode_param_index: number)
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
    local function auto_release()
        return partial_mapping {
            glue = realearn.Glue {
                target_interval = { 0.0, 0.0 },
                fire_mode = realearn.FireMode.Normal {
                    press_duration_interval = { long_press_millis, 10000 },
                },
            },
        }
    end

    local function fire_if_pressed_less_than(millis: number)
        return partial_mapping {
            glue = realearn.Glue {
                fire_mode = realearn.FireMode.Normal {
                    press_duration_interval = { 0, millis },
                },
            },
        }
    end

    local function fire_after_timeout(millis: number)
        return partial_mapping {
            glue = realearn.Glue {
                fire_mode = realearn.FireMode.AfterTimeout {
                    timeout = millis,
                },
            },
        }
    end

    local function set_slot_mode_momentary(mode: Mode)
        return set_mode_momentary(mode, slot_mode_count, params.slot_mode.index)
    end

    local function set_column_mode_toggle(mode: Mode)
        return set_mode_toggle(mode, column_mode_count, params.column_mode.index)
    end

    local function set_column_mode_momentary(mode: Mode)
        return set_mode_momentary(mode, column_mode_count, params.column_mode.index)
    end

    local function slot_mode_is(slot_mode: Mode)
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Expression {
                condition = `p[{params.shift.index}] == 0.0 && p[{params.slot_mode.index}] == {slot_mode.index}`,
            },
        }
    end

    local function column_mode_is_normal_and_slot_mode_is(slot_mode: Mode)
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Expression {
                condition = `p[{params.shift.index}] == 0.0 && p[{params.column_mode.index}] == {column_modes.normal.index} && p[{params.slot_mode.index}] == {slot_mode.index}`,
            },
        }
    end

    local function column_mode_is(column_mode: Mode)
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Bank {
                parameter = params.column_mode.index,
                bank_index = column_mode.index,
            },
        }
    end

    local function shift_is(on: boolean)
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

    local function led_on_off()
        return partial_mapping {
            target = realearn.Target.Dummy {},
        }
    end

    local function discrete_target_interval(min: number, max: number, count: number)
        local max_val = count - 1
        return partial_mapping {
            glue = realearn.Glue {
                target_interval = { min / max_val, max / max_val },
            },
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

    local function shift_config(needs_shift: boolean)
        return shift_is(needs_shift) + (if needs_shift then shift_colors else normal_colors)
    end

    local mappings = {}

    if config.has_shift_button then
        table.insert(mappings, name("Shift") + button("shift") + shift_colors + set_param(params.shift.index))
    end

    if config.has_delete_button then
        table.insert(
            mappings,
            name("Delete")
                + button("delete")
                + shift_is(false)
                + normal_colors
                + set_slot_mode_momentary(slot_modes.delete)
        )
    end

    if config.has_fixed_length_button then
        -- When short-pressing "Fixed length" button, toggle between open ended and custom-length mode
        table.insert(
            mappings,
            name("Fixed length")
                + button("fixed-length")
                + shift_is(false)
                + fire_if_pressed_less_than(long_press_millis)
                + normal_colors
                + toggle()
                + discrete_target_interval(0, 1, 10)
                + clip_matrix_action("SetRecordLengthMode")
        )
        -- When long-pressing "Fixed length" button, allow user to configure custom length via column action buttons
        table.insert(
            mappings,
            name("Fixed length - Long press 1")
                + button("fixed-length")
                + shift_is(false)
                + fire_after_timeout(long_press_millis)
                + feedback_disabled()
                + set_column_mode_momentary(column_modes.set_record_length)
        )
        -- When long-pressing "Fixed length" button, also enter custom-length mode
        table.insert(
            mappings,
            name("Fixed length - Long press 2")
                + button("fixed-length")
                + shift_is(false)
                + fire_after_timeout(long_press_millis)
                + feedback_disabled()
                + discrete_target_interval(1, 1, 10)
                + clip_matrix_action("SetRecordLengthMode")
        )
    end

    if config.has_duplicate_button then
        table.insert(
            mappings,
            name("Duplicate")
                + button("duplicate")
                + shift_is(false)
                + normal_colors
                + set_slot_mode_momentary(slot_modes.duplicate)
        )
    end

    if config.has_play_button then
        table.insert(
            mappings,
            name("Play")
                + button("play")
                + shift_is(false)
                + normal_colors
                + toggle()
                + clip_matrix_action("PlayIgnitedOrEnterSilenceMode")
        )
    end

    if config.has_record_button then
        table.insert(
            mappings,
            name("Record") + button("record") + shift_is(false) + arm_colors + clip_matrix_action("SmartRecord")
        )
    end

    if config.has_quantize_button then
        table.insert(
            mappings,
            name("Quantise")
                + button("quantize")
                + shift_is(false)
                + normal_colors
                + set_slot_mode_momentary(slot_modes.quantise)
        )
        table.insert(
            mappings,
            name("Record Quantise")
                + button("quantize")
                + shift_is(true)
                + shift_colors
                + toggle()
                + clip_matrix_action("MidiAutoQuantizationOnOffState")
        )
    end

    if config.click_button_id then
        table.insert(
            mappings,
            name("Click")
                + button(config.click_button_id)
                + shift_config(config.click_needs_shift)
                + toggle()
                + clip_matrix_action("ClickOnOffState")
        )
    end

    if config.undo_button_id then
        table.insert(
            mappings,
            name("Undo")
                + button(config.undo_button_id)
                + shift_is(config.undo_needs_shift)
                + feedback_disabled()
                + clip_matrix_action("Undo")
        )
        table.insert(
            mappings,
            name("Undo LED") + button(config.undo_button_id) + shift_config(config.undo_needs_shift) + led_on_off()
        )
    end

    if config.redo_button_id then
        table.insert(
            mappings,
            name("Redo")
                + button(config.redo_button_id)
                + shift_is(config.redo_needs_shift)
                + feedback_disabled()
                + clip_matrix_action("Redo")
        )
        table.insert(
            mappings,
            name("Redo LED") + button(config.redo_button_id) + shift_config(config.redo_needs_shift) + led_on_off()
        )
    end

    if config.tap_button_id then
        table.insert(
            mappings,
            name("Tap")
                + button(config.tap_button_id)
                + shift_is(config.tap_needs_shift)
                + feedback_disabled()
                + clip_matrix_action("TapTempo")
        )
        table.insert(
            mappings,
            name("Tap LED") + button(config.tap_button_id) + shift_config(config.tap_needs_shift) + led_on_off()
        )
    end

    if config.double_button_id then
        table.insert(
            mappings,
            name("Double")
                + button(config.double_button_id)
                + shift_config(config.double_needs_shift)
                + set_slot_mode_momentary(slot_modes.double)
        )
    end

    local function add_scroll_mappings(b: string, target: partial.Partial)
        table.insert(mappings, name(b) + button(b) + feedback_disabled() + turbo() + target)
        table.insert(mappings, name(b) + button(b) + control_disabled() + colors(0, 23) + target)
    end

    add_scroll_mappings("cursor-up", scroll_vertically(-1))
    add_scroll_mappings("cursor-down", scroll_vertically(1))
    add_scroll_mappings("cursor-left", scroll_horizontally(-1))
    add_scroll_mappings("cursor-right", scroll_horizontally(1))

    local function add_column_mode_mappings(n: string, b: string, m: Mode, partial: partial.Partial)
        table.insert(mappings, name(n) + button(b) + shift_is(false) + partial + set_column_mode_toggle(m))
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
        if config.use_last_row_play_button_for_column_modes and row == row_count - 1 then
            local function val(mode: Mode): number
                return mode.index / (column_mode_count - 1);
            end
            table.insert(
                mappings,
                name("Switch column mode") + feedback_disabled() + row_play_button(row) + partial_mapping {
                    glue = realearn.Glue {
                        absolute_mode = "IncrementalButton",
                        target_value_sequence = `0, {val(column_modes.stop_clip)}, {val(column_modes.solo)}, {val(column_modes.mute)}`,
                        wrap = true,
                    },
                    target = realearn.Target.CompartmentParameterValue {
                        parameter = realearn.CompartmentParameterDescriptor.ById {
                            index = params.column_mode.index,
                        },
                    },
                }
            )
        else
            table.insert(
                mappings,
                name("Play scene") + feedback_disabled() + row_play_button(row) + clip_row_action(row, "PlayScene")
            )
            table.insert(mappings, name("Play scene LED") + row_play_button(row) + led_on_off())
        end
    end

    -- For each slot
    for col = 0, column_count - 1 do
        for row = 0, row_count - 1 do
            local function create_normal_control_condition(slot_mode: Mode)
                if not config.has_dedicated_column_action_buttons and row == row_count - 1 then
                    -- The last row is special. If the column mode is not the normal one, it will carry out different functions.
                    return column_mode_is_normal_and_slot_mode_is(slot_mode)
                end
                return slot_mode_is(slot_mode)
            end
            local function create_normal_feedback_condition()
                if not config.has_dedicated_column_action_buttons and row == row_count - 1 then
                    -- The last row is special. If the column mode is not the normal one, it will carry out different functions.
                    return column_mode_is(column_modes.normal)
                end
                return partial_mapping {}
            end
            -- Activate slot (shift)
            table.insert(
                mappings,
                name("Activate slot")
                    + shift_is(true)
                    + feedback_disabled()
                    + slot_button(col, row)
                    + clip_management_action(col, row, "Activate")
            )
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
                    + clip_management_action(col, row, "DoubleClipSectionLength")
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

    -- For the last row of slots or - if available - the dedicated column action buttons
    for col = 0, column_count - 1 do
        if config.has_dedicated_column_action_buttons then
            table.insert(
                mappings,
                name("Activate column")
                    + column_mode_is(column_modes.normal)
                    + column_action_button(col)
                    + select_colors
                    + clip_column_action(col, "Activate")
            )
            table.insert(
                mappings,
                name(`Set custom record length to {col + 1} bars`)
                    + column_mode_is(column_modes.set_record_length)
                    + column_action_button(col)
                    + discrete_target_interval(col, col, 64)
                    + clip_matrix_action("SetCustomRecordLengthInBars")
            )
        end

        local source = if config.has_dedicated_column_action_buttons
            then column_action_button(col)
            else slot_button(col, row_count - 1)
        -- Record-arm track
        table.insert(
            mappings,
            name("Record Arm")
                + column_mode_is(column_modes.record_arm)
                + source
                + toggle()
                + arm_colors
                + clip_column_action(col, "ArmStateExclusive")
        )
        -- Select track
        table.insert(
            mappings,
            name("Track Select")
                + column_mode_is(column_modes.track_select)
                + source
                + toggle()
                + select_colors
                + column_track_target(col, "TrackSelectionState", true)
        )
        -- Mute track
        table.insert(
            mappings,
            name("Mute")
                + column_mode_is(column_modes.mute)
                + source
                + toggle()
                + mute_colors
                + column_track_target(col, "TrackMuteState")
        )
        -- Solo track
        table.insert(
            mappings,
            name("Solo")
                + column_mode_is(column_modes.solo)
                + source
                + toggle()
                + solo_colors
                + column_track_target(col, "TrackSoloState")
        )
        -- Stop column
        table.insert(
            mappings,
            name("Stop column")
                + column_mode_is(column_modes.stop_clip)
                + source
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
end

return module
