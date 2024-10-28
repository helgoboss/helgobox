--- name: Console 1 mk2 - Native mode
--- realearn_version: 2.16.12
--- author: helgoboss
--- description: |
---     This controller preset implements support for the Softube Console 1 controller in native mode.
---
---     Pros:
---     * True relative control without glitches
---     * True momentary buttons
---     * LED feedback without glitches
---     * Button "Fine Adjust" is available and can be customized
---
---     Cons:
---     * Button "On" is not available
---     * Using Softube plug-ins that connect to the Console 1 is not possible (they would interfere)
---
--- setup_instructions: |
---     1. Connect the controller
---     2. Start the Console 1 On-Screen Display software
---     3. Make sure to not use any Softube plug-ins
---
--- device_manufacturer: Softube
--- device_name: Console 1 mk2
--- provided_schemes: [softube/console1-mk2]

--!strict

local commons = require("softube/console1-mk2-lib/preset-common")
return commons.create_compartment {
    mode = "native",
}
