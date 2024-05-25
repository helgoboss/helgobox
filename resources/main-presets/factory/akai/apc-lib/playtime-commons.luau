--!strict

local util = require("util")
local realearn = require("realearn")
local playtime_util = require("playtime_util")

local module = {}

export type ApcPlaytimePresetConfig = {
    mute_track_button_id: string,
    arm_track_button_id: string,
    use_column_stop_buttons: boolean,
    stop_column_if_slot_empty: boolean,
    column_count: number,
    row_count: number,
    up_button_id: string,
    down_button_id: string,
    left_button_id: string,
    right_button_id: string,
    volume_button_id: string,
    pan_button_id: string,
    send_button_id: string,
    device_button_id: string,
    stop_all_clips_button_id: string,
    stop_all_clips_need_shift: boolean,
}

function module.create_compartment(config: ApcPlaytimePresetConfig): realearn.Compartment
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

    local column_mode_count = 100
    local knob_mode_count = 100

    -- Column modes

    local column_modes: ModeMap = {
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
    local sorted_column_modes = util.sorted_by_index(column_modes)

    -- Knob modes
    local knob_modes: ModeMap = {
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
    local sorted_knob_modes = util.sorted_by_index(knob_modes)

    -- Parameters

    local params: ParameterMap = {
        shift = {
            index = 0,
            name = "Shift modifier",
        },
        column_mode = {
            index = 1,
            name = "Column mode",
            value_count = column_mode_count,
            value_labels = util.extract_labels(sorted_column_modes),
        },
        knob_mode = {
            index = 2,
            name = "Knob mode",
            value_count = knob_mode_count,
            value_labels = util.extract_labels(sorted_knob_modes),
        },
        send = {
            index = 3,
            name = "Send",
            value_count = 2,
        },
        sustain = {
            index = 4,
            name = "Sustain modifier",
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

    local function multi(index: number)
        return partial_mapping {
            source = realearn.Source.Virtual {
                character = "Multi",
                id = index,
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

    local function column_stop_button(col: number)
        return button(`col{col + 1}/stop`)
    end

    local function row_play_button(row: number)
        return button(`row{row + 1}/play`)
    end

    local function slot_button(col: number, row: number)
        return button(`col{col + 1}/row{row + 1}/pad`)
    end

    local function shift_pressed(on: boolean)
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Modifier {
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

    local function sustain_pressed(on: boolean)
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Modifier {
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

    local function shift_or_sustain_pressed()
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Expression {
                condition = `p[{params.shift.index}] || p[{params.sustain.index}]`,
            },
        }
    end

    local function shift_and_sustain_pressed()
        return partial_mapping {
            activation_condition = realearn.ActivationCondition.Modifier {
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

    local function fire_max(millis: number)
        return partial_mapping {
            glue = {
                fire_mode = realearn.FireMode.Normal {
                    press_duration_interval = { 0, millis },
                },
            },
        }
    end

    local function fire_after_timeout(millis: number)
        return partial_mapping {
            glue = {
                fire_mode = realearn.FireMode.AfterTimeout {
                    timeout = millis,
                },
            },
        }
    end

    local function fire_on_single_press(max_duration: number)
        return partial_mapping {
            glue = {
                fire_mode = realearn.FireMode.OnSinglePress {
                    max_duration = max_duration,
                },
            },
        }
    end

    local function fire_on_double_press()
        return partial_mapping {
            glue = {
                fire_mode = realearn.FireMode.OnDoublePress(),
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
    local single_press = fire_on_single_press(200)
    local double_press = fire_on_double_press()

    local function clip_matrix_action(action: realearn.PlaytimeMatrixAction)
        return partial_mapping {
            target = realearn.Target.PlaytimeMatrixAction {
                action = action,
            },
        }
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

    local function route_target(col: number, route_target_kind: realearn.TargetKind)
        return partial_mapping {
            target = {
                kind = route_target_kind,
                route = realearn.RouteDescriptor.Dynamic {
                    track = clip_column_track(col),
                    expression = `p[{params.send.index}]`,
                },
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
            target = realearn.Target.PlaytimeSlotManagementAction {
                slot = create_slot_selector(col, row),
                action = action,
            },
        }
    end

    local function transport_action(action: realearn.TransportAction)
        return partial_mapping {
            target = realearn.Target.TransportAction {
                action = action,
            },
        }
    end

    local function reaper_action(command: realearn.ReaperCommand)
        return partial_mapping {
            target = realearn.Target.ReaperAction {
                command = command,
                invocation = "Trigger",
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

    local function pick_up()
        return partial_mapping {
            glue = {
                takeover_mode = "PickUpTolerant",
            },
        }
    end

    local function incremental()
        return partial_mapping {
            glue = {
                absolute_mode = "IncrementalButton",
            },
        }
    end

    local function wrap()
        return partial_mapping {
            glue = {
                wrap = true,
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

    local function group(g: realearn.Group)
        return partial_mapping {
            group = g.id,
        }
    end

    local function set_mode(mode: Mode, mode_count: number, mode_param_index: number)
        local target_value = mode.index / (mode_count - 1)
        return partial_mapping {
            name = mode.label,
            glue = {
                target_interval = { target_value, target_value },
                out_of_range_behavior = "Min",
            },
            target = realearn.Target.CompartmentParameterValue {
                parameter = realearn.CompartmentParameterDescriptor.ById {
                    index = mode_param_index,
                },
            },
        }
    end

    local function set_column_mode(mode: Mode)
        return set_mode(mode, column_mode_count, params.column_mode.index)
    end

    local function set_knob_mode(mode: Mode)
        return set_mode(mode, knob_mode_count, params.knob_mode.index)
    end

    local function column_mode_is(column_mode: Mode)
        return realearn.ActivationCondition.Bank {
            parameter = params.column_mode.index,
            bank_index = column_mode.index,
        }
    end

    local function knob_mode_is(knob_mode)
        return realearn.ActivationCondition.Bank {
            parameter = params.knob_mode.index,
            bank_index = knob_mode.index,
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
    util.set_keys_as_ids(groups)

    -- Mappings

    local mappings = {
        name("Play/stop") + no_mod + button("play") + toggle() + transport_action("PlayStop"),
        name("Shift modifier") + button("shift") + set_param(params.shift.index),
        name("Sustain modifier") + button("sustain") + set_param(params.sustain.index),
        name("Undo") + shift + button("play") + clip_matrix_action("Undo"),
        name("Redo") + shift + button("record") + clip_matrix_action("Redo"),
        name("Build scene") + sustain + button("record") + clip_matrix_action("BuildScene"),
        name("Switch send")
            + group(groups.knob_sends)
            + feedback_disabled()
            + shift
            + button("col7/stop")
            + incremental()
            + wrap()
            + set_param(params.send.index),
        name("Column stop mode")
            + group(groups.column_modes)
            + shift
            + button("row1/play")
            + set_column_mode(column_modes.stop),
        name("Column solo mode")
            + group(groups.column_modes)
            + shift
            + button("row2/play")
            + set_column_mode(column_modes.solo),
        name("Column arm mode")
            + group(groups.column_modes)
            + shift
            + button(config.arm_track_button_id)
            + set_column_mode(column_modes.record_arm),
        name("Column mute mode")
            + group(groups.column_modes)
            + shift
            + button(config.mute_track_button_id)
            + set_column_mode(column_modes.mute),
        name("Column select mode")
            + group(groups.column_modes)
            + shift
            + button("row5/play")
            + set_column_mode(column_modes.select),
        name("Record 1 bar")
            + group(groups.record_settings)
            + shift_and_sustain
            + button("row1/play")
            + clip_matrix_action("SetRecordDurationToOneBar"),
        name("Record 2 bars")
            + group(groups.record_settings)
            + shift_and_sustain
            + button("row2/play")
            + clip_matrix_action("SetRecordDurationToTwoBars"),
        name("Record 4 bars")
            + group(groups.record_settings)
            + shift_and_sustain
            + button("row3/play")
            + clip_matrix_action("SetRecordDurationToFourBars"),
        name("Record 8 bars")
            + group(groups.record_settings)
            + shift_and_sustain
            + button("row4/play")
            + clip_matrix_action("SetRecordDurationToEightBars"),
        name("Record open-ended")
            + group(groups.record_settings)
            + shift_and_sustain
            + button(config.stop_all_clips_button_id)
            + clip_matrix_action("SetRecordDurationToOpenEnd"),
    }

    if config.stop_all_clips_need_shift then
        -- We have a device that has a "Stop all clips" button but it needs shift to be pressed
        table.insert(
            mappings,
            name("Stop all clips") + shift + button(config.stop_all_clips_button_id) + clip_matrix_action("Stop")
        )
    else
        -- We have a device with a dedicated "Stop all clips" button
        table.insert(
            mappings,
            name("Stop all clips") + no_mod + button(config.stop_all_clips_button_id) + clip_matrix_action("Stop")
        )
        table.insert(
            mappings,
            name("Click") + shift + button(config.stop_all_clips_button_id) + reaper_action(40364)
        )
    end

    if config.use_column_stop_buttons then
        -- Scrolling
        table.insert(
            mappings,
            name("Scroll up") + shift_or_sustain + button(config.up_button_id) + turbo() + scroll_vertically(-1)
        )
        table.insert(
            mappings,
            name("Scroll down") + shift_or_sustain + button(config.down_button_id) + turbo() + scroll_vertically(1)
        )
        table.insert(
            mappings,
            name("Scroll left") + shift_or_sustain + button(config.left_button_id) + turbo() + scroll_horizontally(-1)
        )
        table.insert(
            mappings,
            name("Scroll right") + shift_or_sustain + button(config.right_button_id) + turbo() + scroll_horizontally(1)
        )
        -- Modes
        table.insert(
            mappings,
            name("Knob volume mode")
                + group(groups.knob_modes)
                + shift
                + button(config.volume_button_id)
                + set_knob_mode(knob_modes.volume)
        )
        table.insert(
            mappings,
            name("Knob pan mode")
                + group(groups.knob_modes)
                + shift
                + button(config.pan_button_id)
                + set_knob_mode(knob_modes.pan)
        )
        table.insert(
            mappings,
            name("Knob send mode")
                + group(groups.knob_modes)
                + shift
                + button(config.send_button_id)
                + set_knob_mode(knob_modes.sends)
        )
        table.insert(
            mappings,
            name("Knob device mode")
                + group(groups.knob_modes)
                + shift
                + button(config.device_button_id)
                + set_knob_mode(knob_modes.device)
        )
    end

    -- For each column
    for col = 0, config.column_count - 1 do
        -- Column stop button functions
        if config.use_column_stop_buttons then
            table.insert(
                mappings,
                name("Stop column")
                    + group(groups.column_stop)
                    + no_mod
                    + column_stop_button(col)
                    + clip_column_action(col, "Stop")
            )
            table.insert(
                mappings,
                name("Solo track")
                    + group(groups.column_solo)
                    + no_mod
                    + toggle()
                    + column_stop_button(col)
                    + column_track_target(col, "TrackSoloState")
            )
            table.insert(
                mappings,
                name("Arm track")
                    + group(groups.column_record_arm)
                    + no_mod
                    + toggle()
                    + column_stop_button(col)
                    + clip_column_action(col, "ArmStateExclusive")
            )
            table.insert(
                mappings,
                name("Mute track")
                    + group(groups.column_mute)
                    + no_mod
                    + toggle()
                    + column_stop_button(col)
                    + column_track_target(col, "TrackMuteState")
            )
            table.insert(
                mappings,
                name("Select track")
                    + group(groups.column_select)
                    + no_mod
                    + toggle()
                    + column_stop_button(col)
                    + column_track_target(col, "TrackSelectionState", true)
            )
        end
        -- Knob functions
        table.insert(
            mappings,
            name("Track volume") + group(groups.knob_volume) + multi(col) + pick_up() + column_track_target(col, "TrackVolume")
        )
        table.insert(
            mappings,
            name("Track pan") + group(groups.knob_pan) + multi(col) + pick_up() + column_track_target(col, "TrackPan")
        )
        table.insert(
            mappings,
            name("Track send volume") + group(groups.knob_sends) + multi(col) + pick_up() + route_target(col, "RouteVolume")
        )
    end

    -- For each row
    for row = 0, config.row_count - 1 do
        table.insert(
            mappings,
            name("Play scene")
                + group(groups.row_play_scene)
                + feedback_disabled()
                + no_mod
                + row_play_button(row)
                + clip_row_action(row, "PlayScene")
        )
        table.insert(
            mappings,
            name("Copy or paste")
                + group(groups.row_copy_or_paste_scene)
                + sustain
                + short_press
                + row_play_button(row)
                + clip_row_action(row, "CopyOrPasteScene")
        )
        table.insert(
            mappings,
            name("Long = Clear")
                + group(groups.row_clear_scene)
                + feedback_disabled()
                + sustain
                + long_press
                + row_play_button(row)
                + clip_row_action(row, "ClearScene")
        )
    end

    -- For each slot
    for col = 0, config.column_count - 1 do
        for row = 0, config.row_count - 1 do
            -- Feedback
            table.insert(
                mappings,
                name("Slot feedback")
                    + group(groups.slot_feedback)
                    + control_disabled()
                    + slot_button(col, row)
                    + playtime_util.slot_state_text_feedback()
                    + clip_transport_action(col, row, "Trigger")
            )
            -- Control
            table.insert(
                mappings,
                name("Slot trigger")
                    + group(groups.slot_play)
                    + feedback_disabled()
                    + no_mod
                    + slot_button(col, row)
                    + clip_transport_action(col, row, "Trigger")
            )
            table.insert(
                mappings,
                name("Copy or paste")
                    + group(groups.slot_copy_or_paste)
                    + feedback_disabled()
                    + sustain
                    + single_press
                    + slot_button(col, row)
                    + toggle()
                    + clip_management_action(col, row, "CopyOrPasteClip")
            )
            table.insert(
                mappings,
                name("Long = Delete")
                    + group(groups.slot_clear)
                    + feedback_disabled()
                    + sustain
                    + long_press
                    + slot_button(col, row)
                    + clip_management_action(col, row, "ClearSlot")
            )
            table.insert(
                mappings,
                name("2x = Edit")
                    + group(groups.slot_quantize)
                    + feedback_disabled()
                    + sustain
                    + double_press
                    + slot_button(col, row)
                    + toggle()
                    + clip_management_action(col, row, "EditClip")
            )
            --table.insert(mappings, name("Overdub clip") + group(groups.slot_play) + feedback_disabled() + shift + single_press + slot_button(col, row) + toggle() + clip_transport_action(col, row, "RecordStop", false))
            --table.insert(mappings, name("2x = Double section") + group(groups.slot_double) + feedback_disabled() + shift + double_press + slot_button(col, row) + adjust_clip_section_length_action(col, row, 2))
            --table.insert(mappings, name("1x = Halve section") + group(groups.slot_double) + feedback_disabled() + shift + single_press + slot_button(col, row) + adjust_clip_section_length_action(col, row, 0.5))
            table.insert(
                mappings,
                name("Fill slot")
                    + group(groups.slot_quantize)
                    + feedback_disabled()
                    + shift
                    + single_press
                    + slot_button(col, row)
                    + clip_management_action(col, row, "FillSlotWithSelectedItem")
            )
        end
    end

    return realearn.Compartment {
        parameters = util.sorted_by_index(params),
        groups = util.to_array(groups),
        mappings = mappings :: any,
        custom_data = {
            playtime = {
                control_unit = {
                    column_count = config.column_count,
                    row_count = config.row_count,
                },
            },
        },
    }
end

return module
