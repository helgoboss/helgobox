-- Configuration

local column_count = 8

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
    local human_col = col + 1
    local two_digit_col = format_as_two_digits(human_col)
    local mapping = {
        source = {
            kind = "Virtual",
            id = "col" .. human_col .. "/stop",
            character = "Button",
        },
        glue = {
            absolute_mode = "ToggleButton",
        },
        target = {
            kind = "TrackArmState",
            track = {
                address = "ByName",
                name ="Instrument " .. two_digit_col .. "*",
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