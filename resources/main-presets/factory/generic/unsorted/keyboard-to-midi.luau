local mappings = {}

for i = 0, 7 do
    local m = {
        name = tostring(i + 1),
        source = {
            kind = "Key",
            keystroke = {
                modifiers = 1,
                key = 49 + i,
            },
        },
        glue = {
            button_filter = "PressOnly",
        },
        target = {
            kind = "SendMidi",
            message = "B0 00 0" .. i,
        },
    }
    table.insert(mappings, m)
end

return {
    kind = "MainCompartment",
    value = {
        mappings = mappings,
    }
}
