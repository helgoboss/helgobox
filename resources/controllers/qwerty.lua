local mappings = {}

function key_mapping(modifiers, ascii_code)
    return {
        source = {
            kind = "Key",
            keystroke = {
                modifiers = modifiers,
                key = ascii_code,
            },
        },
        target = {
            kind = "Virtual",
            character = "Button",
            id = "key/" .. string.lower(string.char(ascii_code)),
        },
    }
end

-- Digits
for i = 48, 57 do
    table.insert(mappings, key_mapping(1, i))
end

-- Letters
for i = 65, 90 do
    table.insert(mappings, key_mapping(1, i))
end

-- Comma and dot
table.insert(mappings, key_mapping(0, 44))
table.insert(mappings, key_mapping(0, 46))

return {
    kind = "ControllerCompartment",
    value = {
        mappings = mappings,
    },
}