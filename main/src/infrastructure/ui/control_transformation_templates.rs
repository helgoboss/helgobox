use crate::infrastructure::ui::ControlStyle::{Button, RangeElement};
use crate::infrastructure::ui::{ScriptTemplate, ScriptTemplateGroup};

pub const CONTROL_TRANSFORMATION_TEMPLATES: &[ScriptTemplateGroup] = &[
    ScriptTemplateGroup {
        name: "Normal",
        templates: &[
            ScriptTemplate {
                name: "Reverse",
                content: "y = 1 - x",
                description: "Simple formula which has the same effect as the 'Reverse' checkbox.",
                control_styles: &[RangeElement, Button],
                min_realearn_version: None,
            },
            ScriptTemplate {
                name: "Exponential curve",
                content: "y = pow(x, 8)",
                description: "Simple exponential curve.",
                control_styles: &[RangeElement],
                min_realearn_version: None,
            },
        ],
    },
    ScriptTemplateGroup {
        name: "Transitions",
        templates: &[
            ScriptTemplate {
                name: "Sinus LFO",
                content: "y = (sin(rel_time / 500) + 1) / 2",
                description: r#"TODO"#,
                control_styles: &[Button],
                min_realearn_version: None,
            },
            ScriptTemplate {
                name: "Debouncing press/release",
                content: "y = y == 0 ? 1 : (rel_time < 200 ? none : stop(0))",
                description: "Keep target 'on' as long as moving fader and switch if 'off' after \
            not moving it for some time. To be used with fader/knob/encoder. Good for the new \
            mouse target to simulate a mouse drag, but also good with \
            'Track: Set automation touch state' target to implement an auto-touch/release when \
            you don't have a touch-sensitive fader at hand.",
                control_styles: &[Button],
                min_realearn_version: None,
            },
        ],
    },
];
