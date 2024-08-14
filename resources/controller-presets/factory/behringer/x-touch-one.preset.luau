--- name: X-Touch One
--- realearn_version: 2.16.0
--- author: helgoboss
--- description: |
---     This controller preset implements support for the Behringer X-Touch One controller in MCU mode.
---
---     The MC modes cover all buttons and work great with the "Mackie Control" preset. A potential
---     downside is that the device itself controls the "Master" button, so you can’t use that button in
---     ReaLearn to customize its logic.
--- setup_instructions: |
---     Select the correct mode by holding the stop button and pressing the encoder:
---     
---     - If you have firmware < 1.10 (the firmware is displayed at startup), use mode "MC Std"
---     - If you have at least firmware 1.10, use mode "MC user"
--- device_manufacturer: Behringer
--- device_name: X-Touch One
--- provided_schemes: [daw]

--!strict

local commons = require("mackie/control-universal-lib/preset-common")

return commons.create_compartment {
    companion_data = nil,
    support_x_touch_colors = true,
}