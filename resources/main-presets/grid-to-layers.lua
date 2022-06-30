-- Configuration

local column_count = 8

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

local mappings = {}

for col = 0, column_count - 1 do
    for human_row, layer in ipairs(layers) do
        local human_col = col + 1
        local two_digit_col = format_as_two_digits(human_col)
        local mapping = {
            name = layer .. " " .. human_col,
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

return {
    kind = "MainCompartment",
    value = {
        mappings = mappings,
    },
}