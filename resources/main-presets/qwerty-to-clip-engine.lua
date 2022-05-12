local column_key_sets = {
    { "1", "q", "a", "z", },
    { "2", "w", "s", "d", },
    { "3", "e", "d", "c", },
    { "4", "r", "f", "v", },
    { "5", "t", "g", "b", },
    { "6", "y", "h", "n", },
    { "7", "u", "j", "m", },
    { "8", "i", "k", "comma", },
}

function scene_play(key, row_index)
    return {
        source = {
            kind = "Virtual",
            character = "Button",
            id = "key/" .. key,
        },
        target = {
            kind = "ClipRowAction",
            row = {
                address = "ByIndex",
                index = row_index,
            },
            action = "PlayScene",
        },
    }
end

local mappings = {
    scene_play("9", 0),
    scene_play("o", 1),
    scene_play("l", 2),
    scene_play("period", 3),
}



for col, column_key_set in ipairs(column_key_sets) do
    for row, row_key in ipairs(column_key_set) do
        local mapping = {
            source = {
                kind = "Virtual",
                character = "Button",
                id = "key/" .. row_key,
            },
            glue = {
                --absolute_mode = "Toggle",
            },
            target = {
                kind = "ClipTransportAction",
                slot = {
                    address = "ByIndex",
                    column_index = col - 1,
                    row_index = row - 1,
                },
                action = "PlayStop",
                stop_column_if_slot_empty = true,
                play_start_timing = {
                    kind = "Immediately"
                },
                play_stop_timing = {
                    kind = "Immediately"
                },
            },
        }
        table.insert(mappings, mapping)
    end
end

return {
    kind = "MainCompartment",
    value = {
        mappings = mappings,
    },
}