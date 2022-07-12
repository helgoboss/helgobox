-- Single buttons
local mappings = {
    {
        id = "9f61972c-be41-4f07-a198-0e1af81c639a",
        name = "Up",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 91,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "cursor-up",
            character = "Button",
        },
    },
    {
        id = "21eb1b34-e946-4f2b-9907-dbd374030c9f",
        name = "Down",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 92,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "cursor-down",
            character = "Button",
        },
    },
    {
        id = "264ce6cc-1a3d-4915-9902-d322cbaec013",
        name = "Left",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 93,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "cursor-left",
            character = "Button",
        },
    },
    {
        id = "094b4846-c00d-48b6-8cd4-0b72a68a7e48",
        name = "Right",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 94,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "cursor-right",
            character = "Button",
        },
    },
    {
        id = "81df76e2-875e-4341-9729-4b67f5246afc",
        name = "Session",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 95,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "session",
            character = "Button",
        },
    },
    {
        id = "b72d8baa-11ac-4e57-a241-48183eb82fff",
        name = "Note",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 96,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "note",
            character = "Button",
        },
    },
    {
        id = "d469f9b4-f4c1-4190-a7ba-3071fb78b195",
        name = "Device",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 97,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "device",
            character = "Button",
        },
    },
    {
        id = "45caeaf1-d8e0-4ae0-8113-6c04368e78c6",
        name = "User",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 98,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "user",
            character = "Button",
        },
    },
    {
        id = "399c2a04-07fd-4e21-9a48-d4f6451fc281",
        name = "Shift",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 80,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "shift",
            character = "Button",
        },
    },
    {
        id = "8acf4da3-7f1b-461d-9f6f-301d079d7f79",
        name = "Click",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 70,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "click",
            character = "Button",
        },
    },
    {
        id = "6d638c9a-fbc3-4102-b49e-c711cea21cb4",
        name = "Undo",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 60,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "undo",
            character = "Button",
        },
    },
    {
        id = "3e1b38db-77e1-4528-8540-e8dbe9d823c6",
        name = "Delete",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 50,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "delete",
            character = "Button",
        },
    },
    {
        id = "94f114c7-0aeb-4398-ae84-38d99c1ee68b",
        name = "Quantise",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 40,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "quantize",
            character = "Button",
        },
    },
    {
        id = "07081feb-b944-4750-887b-55c417d2d42d",
        name = "Duplicate",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 30,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "duplicate",
            character = "Button",
        },
    },
    {
        id = "23630b03-fe6c-4cb2-b38c-ca0b1d896bba",
        name = "Double",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 20,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "double",
            character = "Button",
        },
    },
    {
        id = "efb25588-f46a-41e9-ae82-4b2ebbc0d744",
        name = "Record",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 10,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "record",
            character = "Button",
        },
    },
    {
        id = "522ac86b-95f3-4faa-9356-6fb005d71fb6",
        name = "Record Arm",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 1,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "record-arm",
            character = "Button",
        },
    },
    {
        id = "f67d62b3-12d3-4929-adcc-5a91e7d2b552",
        name = "Track Select",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 2,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "track-select",
            character = "Button",
        },
    },
    {
        id = "70c4889c-adc6-42e4-94be-4c771ef06c13",
        name = "Mute",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 3,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "mute",
            character = "Button",
        },
    },
    {
        id = "55cb4a1b-00f4-4829-834c-44526b66b587",
        name = "Solo",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 4,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "solo",
            character = "Button",
        },
    },
    {
        id = "9925f090-a15b-4437-ae82-0b62420f9d96",
        name = "Volume",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 5,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "volume",
            character = "Button",
        },
    },
    {
        id = "a6790796-8fb1-4d40-a2e8-58fa268fb28c",
        name = "Pan",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 6,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "pan",
            character = "Button",
        },
    },
    {
        id = "0520cdca-6297-4d97-8b6a-16f3585551ef",
        name = "Sends",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 7,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "sends",
            character = "Button",
        },
    },
    {
        id = "5e3e61e2-bd94-4637-96ac-b177010cf152",
        name = "Stop Clip",
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 8,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = "stop-clip",
            character = "Button",
        },
    },
}

-- Clip launch buttons
local feedback_value_table = {
    kind = "FromTextToDiscrete",
    value = {
        -- Off
        empty = 0,
        -- Yellow
        stopped = 1,
        -- Green blinking
        scheduled_for_play_start = 15,
        -- Green
        playing = 17,
        -- Yellow
        paused = 5,
        -- Yellow blinking
        scheduled_for_play_stop = 15,
        -- Red blinking
        scheduled_for_record_start = 5,
        -- Red
        recording = 6,
        -- Yellow blinking
        scheduled_for_record_stop = 5,
    }
}

for col = 0, 7 do
    local human_col = col + 1
    for row = 0, 7 do
        local human_row = row + 1
        local key_number_offset = 11 + (7 - row) * 10
        local id = "col" .. human_col .. "/row" .. human_row .. "/pad"
        local mapping = {
            id = id,
            source = {
                kind = "MidiNoteVelocity",
                channel = 0,
                key_number = key_number_offset + col,
            },
            glue = {
                feedback_value_table = feedback_value_table,
            },
            target = {
                kind = "Virtual",
                id = id,
                character = "Button",
            },
        }
        table.insert(mappings, mapping)
    end
end

-- Scene launch buttons
for row = 0, 7 do
    local human_row = row + 1
    local id = "row" .. human_row .. "/play"
    local mapping = {
        id = id,
        source = {
            kind = "MidiControlChangeValue",
            channel = 0,
            controller_number = 19 + (7 - row) * 10,
            character = "Button",
            fourteen_bit = false,
        },
        target = {
            kind = "Virtual",
            id = id,
            character = "Button",
        },
    }
    table.insert(mappings, mapping)
end

return {
    kind = "ControllerCompartment",
    value = {
        parameters = parameters,
        mappings = mappings,
    },
}