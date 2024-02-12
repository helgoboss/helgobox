--- name: Midi Fighter Twister - Grid
--- author: helgoboss
--- description: |
---     This controller preset exposes the push encoders of the Midi Fighter Twister with the "grid" scheme, which means
---     it uses virtual element IDs such as "col1/row1/pad". This makes it compatible with main presets that are
---     built for generic grid controllers.
--- setup_instructions: |
---     The Midi Fighter Twister is a very customizable controller. Please follow the instructions at
---     https://github.com/helgoboss/realearn/blob/master/doc/controllers.adoc#dj-techtools-midi-fighter-twister
---     to make it ready for ReaLearn.
--- device_manufacturer: DJ TechTools
--- device_name: Midi Fighter Twister
--- midi_identity_pattern: F0 7E ? 06 02 00 01 79 05 00 01 00 * F7
--- realearn_version: 2.16.0-pre.8
--- provided_schemes: [djtechtools/midi-fighter-twister/grid, grid]

--!strict

local realearn = require("realearn")
local commons = require("djtechtools/midi-fighter-twister-lib/preset-commons")

return commons.create_compartment {
    create_push_target = function (col, row)
        return realearn.Target.Virtual {
            id = `col{col +1 }/row{row + 1}/pad`,
            character = "Button",
        }
    end,
    create_turn_target = function (col, row)
        return realearn.Target.Virtual {
            id = `col{col +1 }/row{row + 1}/knob`,
            character = "Multi",
        }
    end,
}