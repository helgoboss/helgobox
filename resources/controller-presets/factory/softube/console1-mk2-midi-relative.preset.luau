--- name: Console 1 mk2 - MIDI relative mode
--- realearn_version: 2.16.12
--- author: helgoboss
--- device_manufacturer: Softube
--- device_name: Console 1 mk2
--- description: |
---     This controller preset implements support for the Softube Console 1 controller in MIDI mode, enabling relative control.
---
---     Pros:
---     * True relative control
---     * Button "On" is available and can be customized
---
---     Cons:
---     * Buttons can't be turned into momentary buttons (because Console1 doesn't send any message on release)
---     * Occasional control and feedback glitches are possible (Console1 encoders send absolute messages in a way that makes
---       it hard to convert them into relative messages)
---     * Reacting to encoder increments (or decrements) only via encoder filter doesn't work
---     * Button "Fine Adjust" doesn't have any effect and can't be customized
---
--- setup_instructions: |
---     1. Connect the controller
---     2. Ensure that the Console 1 On-Screen Display software is **not** running!
---
--- provided_schemes: [softube/console1-mk2]

--!strict

-- Config

local commons = require("softube/console1-mk2-lib/preset-common")
return commons.create_compartment {
    mode = "midi-relative",
}
