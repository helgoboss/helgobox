-- TouchOSC: Simple Mk2 / Matrix

local mappings = {}

local feedback_value_table = {
    kind = "FromTextToContinuous",
    value = {
        ["playtime.slot_state.empty"] = 0.0,
        ["playtime.slot_state.stopped"] = 0.2,
        ["playtime.slot_state.scheduled_for_play_start"] = 0.4,
        ["playtime.slot_state.playing"] = 1.0,
        ["playtime.slot_state.paused"] = 0.3,
        ["playtime.slot_state.scheduled_for_play_stop"] = 0.4,
        ["playtime.slot_state.scheduled_for_play_restart"] = 0.4,
        ["playtime.slot_state.scheduled_for_record_start"] = 0.4,
        ["playtime.slot_state.recording"] = 1.0,
        ["playtime.slot_state.scheduled_for_record_stop"] = 0.4,
    }
}

function multitoggle_button(index)
    return {
        kind = "Osc",
        address = "/4/multitoggle/" .. index,
        argument = {
            index = 0,
        },
    }
end

for col = 0, 7 do
    for row = 0, 7 do
        local mapping = {
            source = multitoggle_button(row * 8 + col + 1),
            glue = {
                -- Standard TouchOSC template tries to be too clever and toggles itself :(
                -- So we use a toggle-always transformation.
                absolute_mode = "Normal",
                control_transformation = "y = y > 0.5 ? 0 : 1",
                feedback = {
                    kind = "Text",
                    text_expression = "{{ target.slot_state.id }}",
                },
                feedback_value_table = feedback_value_table,
            },
            target = {
                kind = "ClipTransportAction",
                slot = {
                    address = "ByIndex",
                    column_index = col,
                    row_index = row,
                },
                action = "RecordPlayStop",
                record_only_if_track_armed = true,
                stop_column_if_slot_empty = true,
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