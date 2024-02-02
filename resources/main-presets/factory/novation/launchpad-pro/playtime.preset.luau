--- name: Launchpad Pro - Playtime
--- author: helgoboss
--- realearn_version: 2.16.0-pre.8
--- used_schemes: [novation/launchpad-pro/live]
--- required_features: [playtime]

--!strict

-- TODO-high Make record and play buttons control Playtime's sequencer

-- Configuration

local use_column_stop_buttons = true

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

-- Parameters

local params: ParameterMap = {
    shift = {
        index = 0,
        name = "Shift modifier",
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

function create_slot_selector(col: number, row: number): realearn.ClipSlotDescriptor
    return realearn.ClipSlotDescriptor.Dynamic {
        column_expression = create_col_expression(col),
        row_expression = create_row_expression(row),
    }
end

function create_column_selector(col: number): realearn.ClipColumnDescriptor
    return realearn.ClipColumnDescriptor.Dynamic {
        expression = create_col_expression(col),
    }
end

function create_row_selector(row: number): realearn.ClipRowDescriptor
    return realearn.ClipRowDescriptor.Dynamic {
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

function clip_column_action(col: number, action: realearn.ClipColumnAction)
    return partial_mapping {
        target = realearn.Target.PlaytimeColumnAction {
            column = create_column_selector(col),
            action = action,
        },
    }
end

function clip_row_action(row: number, action: realearn.ClipRowAction)
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
    action: realearn.ClipTransportAction,
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

function clip_matrix_action(action: realearn.ClipMatrixAction)
    return partial_mapping {
        target = realearn.Target.PlaytimeMatrixAction {
            action = action,
        },
    }
end

function reaper_action(command: realearn.ReaperCommand)
    return partial_mapping {
        target = realearn.Target.ReaperAction {
            command = command,
            invocation = "Trigger",
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

function incremental()
    return partial_mapping {
        glue = {
            absolute_mode = "IncrementalButton",
        },
    }
end

function wrap()
    return partial_mapping {
        glue = {
            wrap = true,
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

function set_mode(mode: Mode, mode_count: number, mode_param_index: number)
    local target_value = mode.index / (mode_count - 1)
    return partial_mapping {
        name = mode.label,
        glue = {
            target_interval = { target_value, target_value },
            out_of_range_behavior = "Min",
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.FxParameterDescriptor.ById {
                index = mode_param_index,
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
    name("Undo") + button("undo") + feedback_disabled() + clip_matrix_action("Undo"),
    name("Undo LED") + button("undo") + colors(0.12, 0.98) + led_on_off(),
    name("Scroll up") + button("cursor-up") + turbo() + scroll_vertically(-1),
    name("Scroll down") + button("cursor-down") + turbo() + scroll_vertically(1),
    name("Scroll left") + button("cursor-left") + turbo() + scroll_horizontally(-1),
    name("Scroll right") + button("cursor-right") + turbo() + scroll_horizontally(1),
}

-- For each row
for row = 0, row_count - 1 do
    table.insert(
        mappings,
        name("Play scene") + feedback_disabled() + row_play_button(row) + clip_row_action(row, "PlayScene")
    )
    table.insert(
        mappings,
        name("Play scene LED") + row_play_button(row) + led_on_off()
    )
end

-- For each slot
for col = 0, column_count - 1 do
    for row = 0, row_count - 1 do
        -- Feedback
        table.insert(
            mappings,
            name("Slot feedback")
                + group(groups.slot_feedback)
                + control_disabled()
                + slot_button(col, row)
                + playtime_util.slot_state_text_feedback()
                + clip_transport_action(col, row, "RecordPlayStop", true)
        )
        -- Control
        table.insert(
            mappings,
            name("Rec/play/stop")
                + group(groups.slot_play)
                + feedback_disabled()
                + slot_button(col, row)
                + toggle()
                + clip_transport_action(col, row, "RecordPlayStop", true)
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
                column_count = column_count,
                row_count = row_count,
            },
        },
    },
}
