-- Preparation
local feedback_value_table = {
    -- Off (hopefully black, depends on MFT utility configuration)
    empty = 0,
    -- Blue
    stopped = 1,
    -- Yellow
    scheduled_for_play_start = 64,
    -- Green
    playing = 43,
    -- Blue
    paused = 1,
    -- Yellow
    scheduled_for_play_stop = 64,
    -- Yellow
    scheduled_for_record_start = 64,
    -- Red
    recording = 86,
    -- Yellow
    scheduled_for_record_stop = 64,
}

function create_cell_id(col, row, id)
    return "col" .. (col + 1) .. "/row" .. (row + 1) .. "/" .. id
end

function pad(col, row, cc_number)
    local id = create_cell_id(col, row, "pad")
    return {
        id = id,
        source = {
            kind = "MidiControlChangeValue",
            channel = 1,
            controller_number = cc_number,
            character = "Button",
        },
        glue = {
            feedback_value_table = feedback_value_table,
        },
        target = {
            kind = "Virtual",
            id = id,
            character = "Button",
        }
    }
end

function knob(col, row, cc_number)
    local id = create_cell_id(col, row, "knob")
    return {
        id = id,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = cc_number,
            character = "Relative2",
        },
        target = {
            kind = "Virtual",
            id = id,
        }
    }
end

function side_button(id, cc_number)
    return {
        id = id,
        feedback_enabled = false,
        source = {
            kind = "MidiControlChangeValue",
            channel = 3,
            controller_number = cc_number,
            character = "Button",
        },
        target = {
            kind = "Virtual",
            id = id,
            character = "Button",
        },
    }
end

-- Side buttons
local mappings = {
    side_button("bank-left", 8),
    side_button("cursor-left", 9),
    side_button("ch-left", 10),
    side_button("bank-right", 11),
    side_button("cursor-right", 12),
    side_button("ch-right", 13),
}

-- Grid
local col_count = 4
local row_count = 4
for col = 0, col_count - 1 do
    for row = 0, row_count - 1 do
        local cc_number = row * row_count + col
        table.insert(mappings, pad(col, row, cc_number))
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
    },
    ["gridDivisionCount"] = 2,
    ["gridSize"] = 50,
}

-- Result
return {
    kind = "ControllerCompartment",
    value = {
        mappings = mappings,
        custom_data = {
            companion = companion_data,
        },
    },
}