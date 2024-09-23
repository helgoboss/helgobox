--!strict

local preset_runtime = require("preset_runtime")
local realearn = require("realearn")

local module = {}

export type MidiFighterTwisterPresetConfig = {
    create_turn_target: (col: number, row: number) -> realearn.Target,
    create_push_target: (col: number, row: number) -> realearn.Target,
}

local common_lua = preset_runtime.include_str("djtechtools/midi-fighter-twister-lib/compartment-common.luau")

local function build_pad_script(pad_index: number): string
    return `return require("compartment").pad_script({pad_index}, y, context)`
end

function module.create_compartment(config: MidiFighterTwisterPresetConfig): realearn.Compartment
    -- Preparation
    local function pad_control(col: number, row: number, cc_number: number): realearn.Mapping
        return realearn.Mapping {
            id = `col{col + 1}/row{row + 1}/pad`,
            feedback_enabled = false,
            source = realearn.Source.MidiControlChangeValue {
                channel = 1,
                controller_number = cc_number,
                character = "Button",
            },
            target = config.create_push_target(col, row),
        }
    end

    local function pad_feedback(col: number, row: number, cc_number: number): realearn.Mapping
        return realearn.Mapping {
            id = `col{col + 1}/row{row + 1}/pad/feedback`,
            control_enabled = false,
            visible_in_projection = false,
            source = realearn.Source.MidiScript {
                script_kind = "Lua",
                script = build_pad_script(cc_number),
            },
            target = config.create_push_target(col, row)
        }
    end

    local function knob(col: number, row: number, cc_number: number): realearn.Mapping
        return realearn.Mapping {
            id = `col{col + 1}/row{row + 1}/knob`,
            source = realearn.Source.MidiControlChangeValue {
                channel = 0,
                controller_number = cc_number,
                character = "Relative2",
            },
            glue = {
                step_factor_interval = { 1, 100 }
            },
            target = config.create_turn_target(col, row),
        }
    end

    local function side_button(id: string, cc_number: number): realearn.Mapping
        return realearn.Mapping {
            id = id,
            feedback_enabled = false,
            source = realearn.Source.MidiControlChangeValue {
                channel = 3,
                controller_number = cc_number,
                character = "Button",
            },
            target = realearn.Target.Virtual {
                id = id,
                character = "Button",
            },
        }
    end

    local mappings: { realearn.Mapping } = {
        -- Side buttons
        side_button("bank-left", 8),
        side_button("cursor-left", 9),
        side_button("ch-left", 10),
        side_button("bank-right", 11),
        side_button("cursor-right", 12),
        side_button("ch-right", 13),
    }

    -- Grid
    local column_count = 4
    local row_count = 4
    for col = 0, column_count - 1 do
        for row = 0, row_count - 1 do
            local cc_number = row * row_count + col
            table.insert(mappings, pad_control(col, row, cc_number))
            table.insert(mappings, pad_feedback(col, row, cc_number))
            table.insert(mappings, knob(col, row, cc_number))
        end
    end

    -- Companion data

    local companion_data = {
        controls = {
            {
                height = 50,
                id = "col4/row1/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col4/row1/knob",
                    "col4/row1/pad",
                },
                shape = "circle",
                width = 50,
                x = 400,
                y = 0,
            },
            {
                height = 50,
                id = "col1/row2/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col1/row2/knob",
                    "col1/row2/pad",
                },
                shape = "circle",
                width = 50,
                x = 100,
                y = 100,
            },
            {
                height = 50,
                id = "col4/row2/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col4/row2/knob",
                    "col4/row2/pad",
                },
                shape = "circle",
                width = 50,
                x = 400,
                y = 100,
            },
            {
                height = 50,
                id = "col1/row1/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col1/row1/knob",
                    "col1/row1/pad",
                },
                shape = "circle",
                width = 50,
                x = 100,
                y = 0,
            },
            {
                height = 50,
                id = "col3/row1/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col3/row1/knob",
                    "col3/row1/pad",
                },
                shape = "circle",
                width = 50,
                x = 300,
                y = 0,
            },
            {
                height = 50,
                id = "col2/row1/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col2/row1/knob",
                    "col2/row1/pad",
                },
                shape = "circle",
                width = 50,
                x = 200,
                y = 0,
            },
            {
                height = 50,
                id = "col2/row2/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col2/row2/knob",
                    "col2/row2/pad",
                },
                shape = "circle",
                width = 50,
                x = 200,
                y = 100,
            },
            {
                height = 50,
                id = "col3/row2/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col3/row2/knob",
                    "col3/row2/pad",
                },
                shape = "circle",
                width = 50,
                x = 300,
                y = 100,
            },
            {
                height = 50,
                id = "col2/row3/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col2/row3/knob",
                    "col2/row3/pad",
                },
                shape = "circle",
                width = 50,
                x = 200,
                y = 200,
            },
            {
                height = 50,
                id = "col3/row3/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col3/row3/knob",
                    "col3/row3/pad",
                },
                shape = "circle",
                width = 50,
                x = 300,
                y = 200,
            },
            {
                height = 50,
                id = "col4/row4/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col4/row4/knob",
                    "col4/row4/pad",
                },
                shape = "circle",
                width = 50,
                x = 400,
                y = 300,
            },
            {
                height = 50,
                id = "col1/row4/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col1/row4/knob",
                    "col1/row4/pad",
                },
                shape = "circle",
                width = 50,
                x = 100,
                y = 300,
            },
            {
                height = 50,
                id = "col2/row4/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col2/row4/knob",
                    "col2/row4/pad",
                },
                shape = "circle",
                width = 50,
                x = 200,
                y = 300,
            },
            {
                height = 50,
                id = "col3/row4/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col3/row4/knob",
                    "col3/row4/pad",
                },
                shape = "circle",
                width = 50,
                x = 300,
                y = 300,
            },
            {
                height = 50,
                id = "col1/row3/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col1/row3/knob",
                    "col1/row3/pad",
                },
                shape = "circle",
                width = 50,
                x = 100,
                y = 200,
            },
            {
                height = 50,
                id = "col4/row3/knob",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "center",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "col4/row3/knob",
                    "col4/row3/pad",
                },
                shape = "circle",
                width = 50,
                x = 400,
                y = 200,
            },
            {
                height = 50,
                id = "bank-right",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "belowBottom",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "bank-right",
                },
                shape = "rectangle",
                width = 50,
                x = 500,
                y = 50,
            },
            {
                height = 50,
                id = "bank-left",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "belowBottom",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "bank-left",
                },
                shape = "rectangle",
                width = 50,
                x = 0,
                y = 50,
            },
            {
                height = 50,
                id = "a78b277e-cfbf-4b2b-9cc6-1a550aeb87fd",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "belowBottom",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "ch-left",
                },
                shape = "rectangle",
                width = 50,
                x = 0,
                y = 250,
            },
            {
                height = 50,
                id = "e312d2a2-ecf1-4189-95af-4174c43a750c",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "belowBottom",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "ch-right",
                },
                shape = "rectangle",
                width = 50,
                x = 500,
                y = 250,
            },
            {
                height = 50,
                id = "cursor-left",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "belowBottom",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "cursor-left",
                },
                shape = "rectangle",
                width = 50,
                x = 0,
                y = 150,
            },
            {
                height = 50,
                id = "cursor-right",
                ["labelOne"] = {
                    angle = 0,
                    position = "aboveTop",
                    ["sizeConstrained"] = true,
                },
                ["labelTwo"] = {
                    angle = 0,
                    position = "belowBottom",
                    ["sizeConstrained"] = true,
                },
                mappings = {
                    "cursor-right",
                },
                shape = "rectangle",
                width = 50,
                x = 500,
                y = 150,
            },
        },
        ["gridDivisionCount"] = 2,
        ["gridSize"] = 50,
    }

    -- Result
    return realearn.Compartment {
        mappings = mappings,
        common_lua = common_lua,
        custom_data = {
            companion = companion_data,
            numbered = {
                multi_count = column_count * row_count,
                button_count = column_count * row_count,
            },
            grid = {
                column_count = column_count,
                row_count = row_count,
            },
        },
    }
end

return module
