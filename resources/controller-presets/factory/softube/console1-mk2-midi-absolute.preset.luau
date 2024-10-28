--- name: Console 1 mk2 - MIDI absolute mode
--- realearn_version: 2.16.12
--- author: helgoboss
--- device_manufacturer: Softube
--- device_name: Console 1 mk2
--- description: |
---     This controller preset implements support for the Softube Console 1 controller in MIDI mode, enabling absolute control.
---
---     Pros:
---     * LED feedback without glitches
---     * Button "On" is available and can be customized
---
---     Cons:
---     * Buttons can't be turned into momentary buttons (because Console1 doesn't send any message on release)
---     * True relative control is not possible (no user-defined step sizes and no wrap)
---     * Without pressing "Fine Adjust", encoder control is very coarse
---     * Button "Fine Adjust" can't be customized (it just switches to fine control on hardware side)
---     * In order to avoid parameter jumps with the encoders, feedback must be enabled (alternatively, you can use 
---       ReaLearn's takeover modes) 
---
--- setup_instructions: |
---     1. Connect the controller
---     2. Ensure that the Console 1 On-Screen Display software is **not** running!
---
--- provided_schemes: [softube/console1-mk2]

--!strict

-- Config

-- With this, we can simulate true relative control.
--
-- In MIDI mode, the encoders of the Console 1 send absolute control values, not relative ones. This is
-- not optimal because it means we can't define step sizes, wrap etc. However, as long as feedback is enabled, at least we don't
-- need to suffer from parameter jumps (a common disadvantage of absolute control).
-- This is because the Console 1 doesn't just use the feedback to set the LED ring, it also resets its internal encoder
-- value to the incoming feedback value.
--
-- If we simulate relative control, we get the advantages of relative control. However, it doesn't
-- work so nicely with feedback :( The "reset-internal-encoder-value" mechanism mentioned above interferes.
-- Without further treatment, this makes the control stuck. We can make control work by not sending echo feedback
-- but then the indicated LED ring value is incorrect when we turn the encoder (however, it is correct when changing the parameter
-- in REAPER itself - as long as we don't disable feedback completely).
-- The technique we use right now is to compare with the previously sent feedback value - if available - and otherwise the previously
-- sent control value. This sort of works, but there can be glitches because it's not always sure that the last-sent feedback value
-- has actually been received by the controller.

local commons = require("softube/console1-mk2-lib/preset-common")
return commons.create_compartment {
    mode = "midi-absolute",
}
