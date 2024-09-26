--!strict

--- Contains types for building ReaLearn feedback scripts in Lua.
local module = {}

--- Provides access to some data that might come in useful to create the feedback value.
export type Context = {
    --- Queries arbitrary properties, e.g. target or mapping properties.
    ---
    --- Those properties are the very same properties that you can use in textual feedback.
    prop: (key: string) -> any?
}

--- You are supposed to return a value structured like that.
export type Output = {
    --- Feedback event.
    feedback_event: FeedbackEvent,
}

--- Feedback event.
export type FeedbackEvent = {
    --- Feedback value (the most important piece of information to return).
    ---
    --- Can either be ...
    ---
    --- - ... a string (ideal for display sources)
    --- - ... a number (ideal for LEDs and motor faders)
    --- - ... nil (which means "turn source off", e.g. turn off the LED, turn down the motor fader, clear the display text)
    --- - ... or anything else: `true`, `false` or an arbitrary table. The latter is currently only useful for the MIDI script source
    ---   because other sources don't support arbitrary tables. 
    value: any?,
    --- Main color.
    ---
    --- Either the default color (= `nil`) or an RGB color.
    color: RgbColor?,
    --- Background color.
    ---
    --- Either the default color (= `nil`) or an RGB color.
    background_color: RgbColor?,
}

--- Color in the RGB color system.
export type RgbColor = {
    r: number,
    g: number,
    b: number,
}

return module