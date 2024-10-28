--- name: Mackie Control Universal
--- realearn_version: 2.16.0
--- author: helgoboss
--- device_manufacturer: Mackie
--- device_name: Control Universal
--- description: |
---     This controller preset implements the Mackie Control Universal (MCU) MIDI protocol. It is supported by many
---     DAW control surfaces out there. I have tested it with Behringer X-Touch One, Behringer X-Touch Compact and
---     iCON Platform M+.
--- setup_instructions: |
---     You probably must put your controller into MCU mode for using this controller preset.
--- provided_schemes: [daw]

--!strict

local commons = require("mackie/control-universal-lib/preset-common")

return commons.create_compartment {
    companion_data = nil,
    support_x_touch_colors = false,
}