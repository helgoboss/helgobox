local mappings = {}

for i = 0, 127 do
    local mapping = {
        source = {
            kind = "Virtual",
            character = "Multi",
            id = i,
        },
        glue = {
            --absolute_mode = "Toggle",
        },
        target = {
            kind = "ClipTransportAction",
            slot = {
                address = "ByIndex",
                column_index = math.floor(i / 12),
                row_index = i % 12,
            },
            action = "PlayStop",
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

return {
    kind = "MainCompartment",
    value = {
        mappings = mappings,
    },
}