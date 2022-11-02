use crate::infrastructure::ui::ControlStyle::{Button, RangeElement};
use crate::infrastructure::ui::{ScriptTemplate, ScriptTemplateGroup};

pub const CONTROL_TRANSFORMATION_TEMPLATES: &[ScriptTemplateGroup] = &[
    ScriptTemplateGroup {
        name: "Normal",
        templates: &[
            ScriptTemplate {
                name: "Reverse",
                content: "y = 1 - x;",
                description: r#"Simple formula which has the same effect as the "Reverse" checkbox."#,
                control_styles: &[RangeElement, Button],
                min_realearn_version: None,
            },
            ScriptTemplate {
                name: "Exponential curve",
                content: "y = pow(x, 8);",
                description: r#"Simple exponential curve."#,
                control_styles: &[RangeElement],
                min_realearn_version: None,
            },
            ScriptTemplate {
                name: "ReaComp Threshold linearization",
                content: r#"// Parameters

min_db = -120;
max_db = 6;

// Code

shift = -min_db;
compress = max_db - min_db;
y = (10.0 ^ ((compress * x - shift) / 20)) / 2"#,
                description: r#"You can use this when controlling the ReaComp Threshold parameter to get a linear dB scale. This essentially inverses the logarithmic curve that ReaComp uses to interpret incoming parameter values. Probably also works with other logarithmic ReaPlug parameters."#,
                control_styles: &[RangeElement],
                min_realearn_version: None,
            },
        ],
    },
    ScriptTemplateGroup {
        name: "Modulations",
        templates: &[
            ScriptTemplate {
                min_realearn_version: None,
                name: "Sinus LFO",
                description: r#"Modulate in sinus shape while button pressed."#,
                control_styles: &[Button],
                content: r#"// Parameters

period = 2;

// Code

y = x > 0 ? (
	secs = rel_time / 1000;
    scaled = secs * ($pi * 2) / period;
    (sin(scaled - $pi / 2) + 1) / 2
) : (
    stop
);"#,
            },
            ScriptTemplate {
                min_realearn_version: None,
                name: "Global sinus LFO",
                description: r#"Modulate in sinus shape while button pressed, continue when button pressed again."#,
                control_styles: &[Button],
                content: r#"// Parameters

period = 2;

// Code

rel_time == 0 ? (
    time_offset += prev_rel_time;
);
prev_rel_time = rel_time;
global_time = time_offset + rel_time;
y = x > 0 ? (
	secs = rel_time / 1000;
    scaled = secs * ($pi * 2) / period;
    (sin(scaled - $pi / 2) + 1) / 2
) : (
    stop
);"#,
            },
        ],
    },
    ScriptTemplateGroup {
        name: "Transitions",
        templates: &[ScriptTemplate {
            min_realearn_version: None,
            name: "Linear transition",
            description: r#"Transition to incoming value (e.g. velocity of button press) within a certain amount of time."#,
            control_styles: &[Button],
            content: r#"// Parameters

transition_time_in_ms = 1000;

// Code

y = abs(x - y) > 0.05 ? (
    x * min(rel_time / transition_time_in_ms, 1)
) : (
    stop
);"#,
        }],
    },
    ScriptTemplateGroup {
        name: "Other",
        templates: &[
            ScriptTemplate {
                name: "Delayed button",
                content: r#"// Parameters

delay_in_ms = 1000;

// Code

y = rel_time < delay_in_ms ? none : stop(x);"#,
                description: "Delays the press/release of a button by a fixed amount of time.",
                control_styles: &[Button],
                min_realearn_version: None,
            },
            ScriptTemplate {
                name: "Debounce",
                content: r#"// Parameters

timeout_in_ms = 200;

// Code

y = y != 1 ? (
    1
) : (
    rel_time < timeout_in_ms ? none : stop(0)
);"#,
                description: "Turns target on as soon as you start moving the knob/fader.\
                Turns it off shortly after you stop moving. \
                Good for the mouse target to simulate a \
            mouse drag but also good with \"Track: Set automation touch state\" target to \
            implement automatic touch/release when you don't have a touch-sensitive fader at hand.",
                control_styles: &[RangeElement],
                min_realearn_version: None,
            },
        ],
    },
];
