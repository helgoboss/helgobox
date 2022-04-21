local column_key_sets = {
    { "1", "q", "a", "z", },
    { "2", "w", "s", "d", },
    { "3", "e", "d", "c", },
    { "4", "r", "f", "v", },
    { "5", "t", "g", "b", },
    { "6", "y", "h", "n", },
    { "7", "u", "j", "m", },
    { "8", "i", "k", ",", },
}

local mappings = {}

for col, column_key_set in ipairs(column_key_sets) do
    for row, row_key in ipairs(column_key_set) do
        local mapping = {
            source = {
                kind = "Virtual",
                character = "Button",
                id = row_key,
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
                action = "RecordPlayStop",
                record_only_if_track_armed = true,
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