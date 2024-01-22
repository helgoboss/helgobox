--- name: Luau example preset
--- author: helgoboss
--- realearn_version: 2.16.0-pre.8
--!strict

local realearn = require("realearn")
-- local realearn = require("realearn.luau")
local Partial = require("Partial")

local a = Partial.new {
    name = "Sustain",
}
local b = Partial.new {
    feedback_enabled = false
}

local mappings: {realearn.Mapping} = {
    {
        id = "22050182-480e-4267-b203-9ad641a72b44",
        name = "Sustain",
        feedback_enabled = false,
        target = {
            kind = "Virtual",
            id = "bla",
            character = "Button",
        },
    },
}

return {
    mappings = mappings,
}