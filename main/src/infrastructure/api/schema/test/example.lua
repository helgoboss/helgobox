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
    visible_in_projection = true,
    enabled = true,
    control_enabled = true,
    feedback_enabled = true,
    active = "Always",
    feedback_behavior = "Normal",
    on_activate = "Normal",
    on_deactivate = "Normal",
    source = MidiNoteVelocityValueSource {
        channel = 0,
        controller_number = 64,
        character = "Button",
        fourteen_bit = false,
    },
    glue = {
        source_interval = {
            0.3,
            0.7
        }
    },
    target = {
        unit = "Percent"
    }
}