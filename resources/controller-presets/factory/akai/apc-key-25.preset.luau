--- name: APC Key 25
--- realearn_version: 2.16.0-pre.8
--- author: helgoboss
--- device_manufacturer: Akai
--- device_name: APC Key 25
--- description: |
---     This controller preset exposes all control elements of the APC Key 25 mk1 in a neutral way. It supports
---     Playtime slot status feedback.
--- midi_identity_pattern: F0 7E ? 06 02 47 27 00 19 * F7
--- provided_schemes: [akai/apc-key-25, grid]

--!strict

-- Configuration

local resolve_shift = false

-- Requires

local realearn = require("realearn")

-- Parameters

local shift_param = realearn.Parameter {
    index = 0,
    name = "Shift",
}
local parameters: { realearn.Parameter }? = if resolve_shift then { shift_param } else nil

-- Single buttons

local function simple_button(id: string, name: string, key_number: number): realearn.Mapping
    return realearn.Mapping {
        id = id,
        name = name,
        feedback_enabled = false,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = key_number,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
end

local mappings = {
    simple_button("stop-all-clips", "SAC", 81),
    simple_button("play", "Play", 91),
    simple_button("record", "Rec", 93),
    realearn.Mapping {
        id = "sustain",
        name = "Sustain",
        feedback_enabled = false,
        source = realearn.Source.MidiControlChangeValue {
            channel = 1,
            controller_number = 64,
            character = "Button",
        },
        target = realearn.Target.Virtual {
            id = "sustain",
            character = "Button",
        },
    },
}

-- Shift
local no_shift_activation_condition: realearn.ActivationCondition?
if resolve_shift then
    -- The activation condition reflecting the state that shift is not pressed.
    no_shift_activation_condition = realearn.ActivationCondition.Modifier {
        modifiers = {
            {
                parameter = 0,
                on = false,
            },
        },
    }
    -- Mapping to make shift button switch to other set of virtual control elements
    local shift_mapping = realearn.Mapping {
        name = "Shift",
        feedback_enabled = false,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 98,
        },
        target = realearn.Target.CompartmentParameterValue {
            parameter = realearn.CompartmentParameterDescriptor.ById {
                index = 0,
            },
        },
    }
    table.insert(mappings, shift_mapping)
    -- Alternative set of virtual control elements
    local alt_elements = {
        { key = 64, id = "cursor-up" },
        { key = 65, id = "cursor-down" },
        { key = 66, id = "cursor-left" },
        { key = 67, id = "cursor-right" },
        { key = 68, id = "volume" },
        { key = 69, id = "pan" },
        { key = 70, id = "sends" },
        { key = 71, id = "device" },
        { key = 82, id = "stop-clip" },
        { key = 83, id = "solo" },
        { key = 84, id = "record-arm" },
        { key = 85, id = "mute" },
        { key = 86, id = "track-select" },
    }
    for _, element in ipairs(alt_elements) do
        local mapping = realearn.Mapping {
            activation_condition = realearn.ActivationCondition.Modifier {
                modifiers = {
                    {
                        parameter = 0,
                        on = true,
                    },
                },
            },
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = element.key,
            },
            target = realearn.Target.Virtual {
                id = element.id,
                character = "Button",
            },
        }
        table.insert(mappings, mapping)
    end
else
    no_shift_activation_condition = nil
    local mapping = simple_button("shift", "Shift", 98)
    table.insert(mappings, mapping)
end

-- Knobs
for i = 0, 7 do
    local human_i = i + 1
    local mapping = realearn.Mapping {
        id = `k{human_i}`,
        feedback_enabled = false,
        source = realearn.Source.MidiControlChangeValue {
            channel = 0,
            controller_number = 48 + i,
        },
        target = realearn.Target.Virtual {
            id = i,
        },
    }
    table.insert(mappings, mapping)
end

-- Clip launch buttons
local feedback_value_table = realearn.FeedbackValueTable.FromTextToDiscrete {
    value = {
        -- Off
        ["playtime.slot_state.empty"] = 0,
        -- Red
        ["playtime.slot_state.armed"] = 0,
        -- Yellow
        ["playtime.slot_state.stopped"] = 5,
        -- Green blinking
        ["playtime.slot_state.scheduled_for_play_start"] = 2,
        -- Green
        ["playtime.slot_state.playing"] = 1,
        -- Yellow
        ["playtime.slot_state.paused"] = 5,
        -- Yellow blinking
        ["playtime.slot_state.scheduled_for_play_stop"] = 6,
        -- Yellow blinking
        ["playtime.slot_state.scheduled_for_play_restart"] = 6,
        -- Red blinking
        ["playtime.slot_state.scheduled_for_record_start"] = 4,
        -- Red
        ["playtime.slot_state.recording"] = 3,
        -- Yellow blinking
        ["playtime.slot_state.scheduled_for_record_stop"] = 6,
    },
}

for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 4 do
        local human_row = row + 1
        local key_number_offset = (4 - row) * 8
        local id = `col{human_col}/row{human_row}/pad`
        local mapping = realearn.Mapping {
            id = id,
            source = realearn.Source.MidiNoteVelocity {
                channel = 0,
                key_number = key_number_offset + col,
            },
            glue = {
                feedback_value_table = feedback_value_table,
            },
            target = realearn.Target.Virtual {
                id = id,
                character = "Multi",
            },
        }
        table.insert(mappings, mapping)
    end
end

-- Clip stop buttons
for col = 0, 7 do
    local human_col = col + 1
    local id = `col{human_col}/stop`
    local mapping = realearn.Mapping {
        id = id,
        activation_condition = no_shift_activation_condition,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 64 + col,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

-- Scene launch buttons
for row = 0, 4 do
    local human_row = row + 1
    local id = `row{human_row}/play`
    local mapping = realearn.Mapping {
        id = id,
        activation_condition = no_shift_activation_condition,
        source = realearn.Source.MidiNoteVelocity {
            channel = 0,
            key_number = 82 + row,
        },
        target = realearn.Target.Virtual {
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

-- Controller layout

local companion_data = {
    controls = {
        {
            height = 32,
            id = "0aa55949-ea14-404e-b3af-3c8a85ed50c7",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col3/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 256,
            y = 0,
        },
        {
            height = 32,
            id = "9b34bea6-9eeb-4a9a-850a-8a8294c772da",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col2/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 128,
            y = 0,
        },
        {
            height = 32,
            id = "bfb2db91-fb28-4b34-9bc4-6b25c18c4534",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col1/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 0,
            y = 0,
        },
        {
            height = 32,
            id = "817b84a8-675f-4edc-8229-67fc3e9a3400",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col4/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 384,
            y = 0,
        },
        {
            height = 32,
            id = "1d094852-f5b9-4aca-9972-71bf7b683b23",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col5/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 512,
            y = 0,
        },
        {
            height = 32,
            id = "7a2fcb32-1e24-4817-a20c-73dbbdd786fc",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col6/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 640,
            y = 0,
        },
        {
            height = 32,
            id = "7ac2b4d1-16b8-43cd-9ebe-56bd375b813f",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col7/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 768,
            y = 0,
        },
        {
            height = 32,
            id = "42836480-6061-4625-81bf-87dba8d45d35",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col8/row1/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 896,
            y = 0,
        },
        {
            height = 32,
            id = "fee5321d-d675-47ec-b4b6-d3e49f04ebda",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col1/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 0,
            y = 64,
        },
        {
            height = 32,
            id = "5760816d-bb92-457e-831c-6ba596d33a91",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col2/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 128,
            y = 64,
        },
        {
            height = 32,
            id = "cb55a16f-f589-4ee5-a977-a19c7ba5d2b2",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col3/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 256,
            y = 64,
        },
        {
            height = 32,
            id = "3b755e21-59c4-4766-9c14-2d20641b7bf4",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col4/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 384,
            y = 64,
        },
        {
            height = 32,
            id = "b6c14173-486f-49f7-87e7-c0fe52bf9898",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col5/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 512,
            y = 64,
        },
        {
            height = 32,
            id = "18b4dfd6-030e-4ddc-9a2a-c88e7270e950",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col6/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 640,
            y = 64,
        },
        {
            height = 32,
            id = "baa813e8-7d4b-4ea7-8610-74f124d3db97",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col7/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 768,
            y = 64,
        },
        {
            height = 32,
            id = "72edace4-890f-446a-b4e8-8d875c1841bc",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col8/row2/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 896,
            y = 64,
        },
        {
            height = 32,
            id = "998c9371-ff24-4954-945d-7c641916500e",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col1/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 0,
            y = 128,
        },
        {
            height = 32,
            id = "c2fdc571-347c-49da-9d9e-6caa4e8d9fcb",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col2/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 128,
            y = 128,
        },
        {
            height = 32,
            id = "6b25f0e2-4d79-40ab-be46-57d49cd8d579",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col3/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 256,
            y = 128,
        },
        {
            height = 32,
            id = "db014d76-4433-463a-ac6f-d38ddb7c364f",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col4/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 384,
            y = 128,
        },
        {
            height = 32,
            id = "8e39bc23-d51d-45c2-ad98-6a609e175584",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col5/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 512,
            y = 128,
        },
        {
            height = 32,
            id = "24cf1dbd-c01e-4a00-baac-282a5e4232af",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col6/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 640,
            y = 128,
        },
        {
            height = 32,
            id = "568c0177-4525-4eee-bfb9-e21ae7c202e0",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col7/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 768,
            y = 128,
        },
        {
            height = 32,
            id = "2ead47d1-ee75-429f-9cd4-3b5fe989c056",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col8/row3/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 896,
            y = 128,
        },
        {
            height = 32,
            id = "941d996d-882c-44f3-abc4-3f6617cc04f9",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col1/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 0,
            y = 192,
        },
        {
            height = 32,
            id = "1ca9b0f7-cded-4f31-adec-3d3cff6c91ca",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col2/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 128,
            y = 192,
        },
        {
            height = 32,
            id = "a7d71b50-81b8-4261-ad8b-7c3229c09d78",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col3/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 256,
            y = 192,
        },
        {
            height = 32,
            id = "4a12fbbd-6ba1-43f2-bf5c-9650076bf376",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col4/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 384,
            y = 192,
        },
        {
            height = 32,
            id = "34b988c8-25e4-4936-8042-7140c7b28dad",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col5/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 512,
            y = 192,
        },
        {
            height = 32,
            id = "bbd4ceba-63d2-441f-8794-0263af223e5c",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col6/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 640,
            y = 192,
        },
        {
            height = 32,
            id = "57a49293-806b-4bff-8dfc-e8130aa8cc23",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col7/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 768,
            y = 192,
        },
        {
            height = 32,
            id = "bcefa1ae-60fe-4e52-b315-645a7b75931e",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col8/row4/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 896,
            y = 192,
        },
        {
            height = 32,
            id = "a1fc5fa6-bfed-4b0a-8132-aed6a4b14bbc",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col1/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 0,
            y = 256,
        },
        {
            height = 32,
            id = "74538f8c-d272-4ac4-a0e2-fcc6452311b5",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col2/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 128,
            y = 256,
        },
        {
            height = 32,
            id = "92bdce70-3a17-4d53-ae93-ec6fdfa4acb1",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col3/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 256,
            y = 256,
        },
        {
            height = 32,
            id = "05ac3407-9209-471e-9b0e-5029396de1cc",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col4/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 384,
            y = 256,
        },
        {
            height = 32,
            id = "b898b127-15e3-4c57-8528-16f5ea7f6af6",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col5/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 512,
            y = 256,
        },
        {
            height = 32,
            id = "d91b3c22-ff5d-4fd0-b8c1-dacd88b17065",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col6/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 640,
            y = 256,
        },
        {
            height = 32,
            id = "99d82570-59ff-4109-aabb-01924bdf5067",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col7/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 768,
            y = 256,
        },
        {
            height = 32,
            id = "161ddee9-7fd6-420d-b9ba-a2714f17c724",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "col8/row5/pad",
            },
            shape = "rectangle",
            width = 96,
            x = 896,
            y = 256,
        },
        {
            height = 32,
            id = "a393009f-655f-4425-9573-8351b151fd3a",
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
                "col1/stop",
            },
            shape = "circle",
            width = 32,
            x = 32,
            y = 320,
        },
        {
            height = 50,
            id = "fb3a1723-7a69-457f-871d-5389c2637f4d",
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
                "col6/stop",
            },
            shape = "circle",
            width = 34,
            x = 672,
            y = 320,
        },
        {
            height = 50,
            id = "44d09c91-a625-4b31-8e1e-e43d44470adc",
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
                "col2/stop",
            },
            shape = "circle",
            width = 34,
            x = 160,
            y = 320,
        },
        {
            height = 50,
            id = "cad2df0b-319c-4c43-88dc-88042b2d6aed",
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
                "col3/stop",
            },
            shape = "circle",
            width = 34,
            x = 288,
            y = 320,
        },
        {
            height = 50,
            id = "70fb4d30-33e8-482b-908f-7d8866d5fbec",
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
                "col4/stop",
            },
            shape = "circle",
            width = 34,
            x = 416,
            y = 320,
        },
        {
            height = 50,
            id = "e92e54a4-96dd-4ed5-9580-e3a587ef86b0",
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
                "col5/stop",
            },
            shape = "circle",
            width = 34,
            x = 544,
            y = 320,
        },
        {
            height = 50,
            id = "1a95adc6-8bb3-4f60-8dfd-7667550b202d",
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
                "col7/stop",
            },
            shape = "circle",
            width = 34,
            x = 800,
            y = 320,
        },
        {
            height = 50,
            id = "ee30202c-0ebd-4abc-bd30-5a961480e5b6",
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
                "col8/stop",
            },
            shape = "circle",
            width = 34,
            x = 928,
            y = 320,
        },
        {
            height = 50,
            id = "0bdeed18-4641-48bd-b7f1-102363c2d1f5",
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
                "row1/play",
            },
            shape = "circle",
            width = 34,
            x = 1024,
            y = 0,
        },
        {
            height = 50,
            id = "39a81c68-dd64-47f0-9d2f-2c36e004b1d1",
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
                "row2/play",
            },
            shape = "circle",
            width = 34,
            x = 1024,
            y = 64,
        },
        {
            height = 50,
            id = "65b0540d-7173-48b9-9b62-eaf833c73e3f",
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
                "row3/play",
            },
            shape = "circle",
            width = 34,
            x = 1024,
            y = 128,
        },
        {
            height = 50,
            id = "4ceafe75-4521-4f87-aabd-cd84d83e46bf",
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
                "row5/play",
            },
            shape = "circle",
            width = 34,
            x = 1024,
            y = 256,
        },
        {
            height = 50,
            id = "8a590659-5632-4227-832c-7bdcbf6caddd",
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
                "row4/play",
            },
            shape = "circle",
            width = 34,
            x = 1024,
            y = 192,
        },
        {
            height = 50,
            id = "fe67392e-15ac-49d8-bcf2-129a3652effb",
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
                "stop-all-clips",
            },
            shape = "circle",
            width = 34,
            x = 1024,
            y = 320,
        },
        {
            height = 50,
            id = "cafb08e0-ba01-4b31-a175-62e4158a42f7",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k1",
            },
            shape = "circle",
            width = 98,
            x = 1120,
            y = 0,
        },
        {
            height = 50,
            id = "f2b031ad-1879-4a75-984c-e906f69bd71e",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k2",
            },
            shape = "circle",
            width = 98,
            x = 1280,
            y = 0,
        },
        {
            height = 50,
            id = "75667a1a-60c8-40e2-95ff-f1f90cc37e92",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k3",
            },
            shape = "circle",
            width = 98,
            x = 1440,
            y = 0,
        },
        {
            height = 50,
            id = "edc1f799-cb0a-4f60-bf8c-b664ab57dc5a",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k4",
            },
            shape = "circle",
            width = 98,
            x = 1600,
            y = 0,
        },
        {
            height = 50,
            id = "5573339f-1524-4ba3-9eb8-238702e437ad",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k5",
            },
            shape = "circle",
            width = 98,
            x = 1120,
            y = 128,
        },
        {
            height = 50,
            id = "9a594e2e-d2b8-4f47-bddc-6872809a4089",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k6",
            },
            shape = "circle",
            width = 98,
            x = 1280,
            y = 128,
        },
        {
            height = 50,
            id = "8b50a75c-c2cd-4f80-9c30-6d4bb9070035",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k7",
            },
            shape = "circle",
            width = 98,
            x = 1440,
            y = 128,
        },
        {
            height = 50,
            id = "9301c8e0-1212-414f-a34e-76af972d68d0",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "k8",
            },
            shape = "circle",
            width = 98,
            x = 1600,
            y = 128,
        },
        {
            height = 34,
            id = "7935e4a0-512f-4e96-8746-b2c8f542c33d",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "sustain",
            },
            shape = "rectangle",
            width = 98,
            x = 1120,
            y = 256,
        },
        {
            height = 34,
            id = "39506af3-b17e-45bb-a148-51ff39b4eeb5",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "play",
            },
            shape = "rectangle",
            width = 98,
            x = 1472,
            y = 256,
        },
        {
            height = 34,
            id = "35aa5f43-6b36-46ad-b104-fbe84c37b96b",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "record",
            },
            shape = "rectangle",
            width = 98,
            x = 1600,
            y = 256,
        },
        {
            height = 34,
            id = "d72b47cc-f0b4-46c6-9510-fd967f815965",
            ["labelOne"] = {
                angle = 0,
                position = "center",
                ["sizeConstrained"] = true,
            },
            ["labelTwo"] = {
                angle = 0,
                position = "belowBottom",
                ["sizeConstrained"] = true,
            },
            mappings = {
                "shift",
            },
            shape = "rectangle",
            width = 98,
            x = 1120,
            y = 320,
        },
    },
    ["gridDivisionCount"] = 2,
    ["gridSize"] = 32,
}

return realearn.Compartment {
    parameters = parameters,
    mappings = mappings,
    custom_data = {
        companion = companion_data,
        grid = {
            column_count = 8,
            row_count = 5,
        },
    },
}
