local mappings = {}

local note_names = {
    "C",
    "C#",
    "D",
    "D#",
    "E",
    "F",
    "F#",
    "G",
    "G#",
    "A",
    "A#",
    "B",
}

for i = 0, 127 do
    local note = {
        name = note_names[i % 12 + 1] .. math.floor(i / 12),
        feedback_enabled = false,
        source = {
            kind = "MidiNoteVelocity",
            key_number = i,
        },
        target = {
            kind = "Virtual",
            character = "Multi",
            id = i
        },
    }
    table.insert(mappings, note)
end

return {
    kind = "ControllerCompartment",
    value = {
        mappings = mappings,
    },
}