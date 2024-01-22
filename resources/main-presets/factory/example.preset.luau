--- name: Luau example preset
--- author: helgoboss
--- realearn_version: 2.16.0-pre.8
--!strict

local common = require("common")
-- local realearn = require("realearn.luau")
local Partial = require("Partial")

local a = Partial.new {
    name = "Sustain",
}
local b = Partial.new {
    feedback_enabled = false
}

local mappings = {
    a + b + {
        id = "22050182-480e-4267-b203-9ad641a72b44",
        source = {
            kind = "MidiControlChangeValue",
            channel = 1,
            controller_number = 64,
            character = "Button",
        },
        target = {
            kind = "Virtual",
            id = "sustain",
            character = "Button",
        },
    },
}

return {
    mappings = mappings,
}