---@class MidiControlChangeValueSource @Responds to CC messages
---
---@field channelNumber number MIDI channel
---@field controllerNumber number MIDI CC number
---@field character "Range"|"Button"|"Relative1"|"Relative2"|"Relative3"|"StatefulButton"
---@field fourteenBit boolean

---@param v MidiControlChangeValueSource
---@return MidiControlChangeValueSource
function MidiNoteVelocityValueSource(v)
    v.type = "MidiControlChangeValue"
    return v
end

---@class Mapping @A ReaLearn mapping.
---
---@field key string Key for referring to that mapping later.
---@field name string Descriptive name for the mapping.
---@field tags table List of tags.
---@field source MidiControlChangeValueSource

---Creates a mapping.
---
---@param v Mapping
---@return Mapping
function Mapping(v) return v end

return Mapping {
    key = "volume",
    name = "Volume",
    tags = {
        "mix",
        "master"
    },
    group = "faders",
    visibleInProjection = true,
    enabled = true,
    controlEnabled = true,
    feedbackEnabled = true,
    active = "Always",
    feedbackBehavior = "Normal",
    onActivate = "Normal",
    onDeactivate = "Normal",
    source = MidiNoteVelocityValueSource {
        channel = 0,
        controllerNumber = 64,
        character = "Button",
        fourteenBit = false,
    },
    glue = {
        sourceInterval = {
            0.3,
            0.7
        }
    },
    target = {
        unit = "Percent"
    }
}