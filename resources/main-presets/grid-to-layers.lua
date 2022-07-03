-- Configuration

local column_count = 8
local row_count = 5

local layers = {
    "Drums",
    "Bass",
    "Filler",
    "Keys",
}

-- Functions

function format_as_two_digits(n)
    if n < 10 then
        return "0" .. tostring(n)
    else
        return tostring(n)
    end
end

-- Mappings

local parameters = {
    {
        index = 0,
        name = "Shift",
    },
}

local groups = {
    {
        id = "slots",
        name = "Slots",
        tags = { "slot" }
    },
    {
        id = "scenes",
        name = "Scenes",
    },
    {
        id = "modifiers",
        name = "Modifiers",
    },
}

local mappings = {
    {
        name = "Shift",
        group = "modifiers",
        source = {
            kind = "Virtual",
            id = "shift",
            character = "Button",
        },
        target = {
            kind = "FxParameterValue",
            parameter = {
                address = "ById",
                fx = {
                    address = "This",
                },
                index = 0,
            },
        },
    }
}

-- Slots

for col = 0, column_count - 1 do
    for human_row, layer in ipairs(layers) do
        local human_col = col + 1
        local two_digit_col = format_as_two_digits(human_col)
        local mapping = {
            name = layer .. " " .. human_col,
            group = "slots",
            source = {
                kind = "Virtual",
                id = "col" .. human_col .. "/row" .. human_row .. "/pad",
                character = "Button",
            },
            glue = {
                source_interval = { 0.04, 1.0 },
                absolute_mode = "ToggleButton",
                reverse = true,
            },
            target = {
                kind = "TrackMuteState",
                track = {
                    address = "ByName",
                    name = layer .. " " .. two_digit_col .. "*",
                },
            },
        }
        table.insert(mappings, mapping)
    end
end

-- Scenes

for row = 0, row_count - 1 do
    local human_row = row + 1
    local save_scene_mapping = {
        name = "Save scene " .. human_row,
        group = "scenes",
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 0,
                    on = true,
                },
            },
        },
        source = {
            kind = "Virtual",
            id = "row" .. human_row .. "/play",
            character = "Button",
        },
        target = {
            kind = "TakeMappingSnapshot",
            tags = {
                "slot",
            },
            active_mappings_only = false,
            snapshot_id = "scene_" .. human_row,
        },
    }
    local load_scene_mapping = {
        name = "Scene " .. human_row,
        group = "scenes",
        activation_condition = {
            kind = "Modifier",
            modifiers = {
                {
                    parameter = 0,
                    on = false,
                },
            },
        },
        source = {
            kind = "Virtual",
            id = "row" .. human_row .. "/play",
            character = "Button",
        },
        target = {
            kind = "LoadMappingSnapshot",
            tags = {
                "slot",
            },
            active_mappings_only = false,
            snapshot = {
                kind = "ById",
                id = "scene_" .. human_row,
            },
            default_value = {
                kind = "Unit",
                value = 1
            }
        },
    }
    table.insert(mappings, save_scene_mapping)
    table.insert(mappings, load_scene_mapping)
end

return {
    kind = "MainCompartment",
    value = {
        parameters = parameters,
        groups = groups,
        mappings = mappings,
    },
}