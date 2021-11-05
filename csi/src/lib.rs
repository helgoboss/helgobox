use derive_more::Display;
use helgoboss_midi::{RawShortMessage, ShortMessage, StructuredShortMessage, U14, U7};
use realearn_api::schema::{
    ApiObject, ButtonFilter, Compartment, Envelope, Glue, MackieLcdSource,
    MackieSevenSegmentDisplayScope, MackieSevenSegmentDisplaySource, Mapping,
    MidiChannelPressureAmountSource, MidiControlChangeValueSource, MidiNoteVelocitySource,
    MidiPitchBendChangeValueSource, MidiPolyphonicKeyPressureAmountSource,
    MidiProgramChangeNumberSource, MidiRawSource, Source, SourceCharacter, Target,
    VirtualControlElementCharacter, VirtualControlElementId, VirtualTarget,
};
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

mod parser;
mod schema;

pub use schema::*;

pub enum CsiObject {
    Widgets(Vec<Widget>),
}

type CsiResult<T> = Result<T, Box<dyn Error>>;

pub fn deserialize_csi_object_from_csi(text: &str) -> Result<CsiObject, Box<dyn Error>> {
    let (_, widgets) = parser::mst_file_content(text).map_err(|e| e.to_string())?;
    Ok(CsiObject::Widgets(widgets))
}

#[derive(Default)]
pub struct Annotator {
    context_stack: Vec<String>,
    annotations: Vec<Annotation>,
}

impl Annotator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_context<R>(&mut self, context: String, f: impl FnOnce(&mut Annotator) -> R) -> R {
        self.context_stack.push(context);
        let result = f(self);
        self.context_stack.pop();
        result
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.annotate(message, AnnotationLevel::Info);
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.annotate(message, AnnotationLevel::Warn);
    }

    fn annotate(&mut self, message: impl Into<String>, level: AnnotationLevel) {
        let annotation = Annotation {
            context_stack: self.context_stack.clone(),
            message: message.into(),
            level,
        };
        self.annotations.push(annotation);
    }

    pub fn build_result<T>(self, value: T) -> AnnotatedResult<T> {
        AnnotatedResult {
            value,
            annotations: self.annotations,
        }
    }
}

#[derive(Display)]
enum AnnotationLevel {
    #[display(fmt = "INFO")]
    Info,
    #[display(fmt = "WARN")]
    Warn,
}

pub struct Annotation {
    context_stack: Vec<String>,
    level: AnnotationLevel,
    message: String,
}

impl Display for Annotation {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let context_expression = self.context_stack.join(" => ");
        write!(f, "{} {}: {}", self.level, context_expression, self.message)
    }
}

pub struct AnnotatedResult<T> {
    pub value: T,
    pub annotations: Vec<Annotation>,
}

impl<T> AnnotatedResult<T> {
    pub fn without_annotations(value: T) -> Self {
        Self {
            value,
            annotations: vec![],
        }
    }
}

impl CsiObject {
    pub fn try_into_api_object(self) -> Result<AnnotatedResult<ApiObject>, Box<dyn Error>> {
        let mut annotator = Annotator::new();
        use CsiObject::*;
        let api_object = match self {
            Widgets(widgets) => {
                let results: Vec<_> = widgets
                    .into_iter()
                    .filter_map(|w| {
                        annotator.with_context(format!("Widget \"{}\"", w.name), |annotator| {
                            match convert_widget(w, annotator) {
                                Ok(res) => Some(res),
                                Err(e) => {
                                    annotator.warn(e.to_string());
                                    None
                                }
                            }
                        })
                    })
                    .collect();
                let has_duplicate_widget_ids = {
                    let id_set: HashSet<_> = results.iter().map(|r| r.widget_id.clone()).collect();
                    results.len() != id_set.len()
                };
                if has_duplicate_widget_ids {
                    annotator.warn("Duplicate widget IDs were produced because of truncation. This will most likely lead to problems! Please shorten the affected widget names.")
                }
                let mappings = results.into_iter().flat_map(|r| r.mappings).collect();
                let compartment = Compartment {
                    mappings: Some(mappings),
                    ..Default::default()
                };
                ApiObject::ControllerCompartment(Envelope {
                    value: Box::new(compartment),
                })
            }
        };
        Ok(annotator.build_result(api_object))
    }
}

struct WidgetConvResult {
    widget_id: String,
    mappings: Vec<Mapping>,
}

fn convert_widget(widget: Widget, annotator: &mut Annotator) -> CsiResult<WidgetConvResult> {
    let widget_name = widget.name;
    let widget_id = convert_widget_name_to_id(&widget_name, annotator)?;
    let mappings = widget
        .capabilities
        .into_iter()
        .flat_map(|c| {
            annotator.with_context(format!("Capability \"{}\"", c), |annotator| {
                convert_capability_to_mappings(&widget_name, &widget_id, c, annotator)
                    .unwrap_or_else(|e| {
                        annotator.info(e.to_string());
                        vec![]
                    })
            })
        })
        .collect();
    let res = WidgetConvResult {
        widget_id,
        mappings,
    };
    Ok(res)
}

fn convert_capability_to_mappings(
    widget_name: &str,
    widget_id: &str,
    capability: Capability,
    annotator: &mut Annotator,
) -> CsiResult<Vec<Mapping>> {
    let base_mapping = Mapping {
        key: Some(format!("{}-{}", widget_id, capability)),
        name: Some(format!("{} - {}", widget_name, capability)),
        ..Default::default()
    };
    let target_character = if capability.is_virtual_button() {
        VirtualControlElementCharacter::Button
    } else {
        VirtualControlElementCharacter::Multi
    };
    let mappings = match capability {
        Capability::Press { press, release } => {
            let press_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: press,
                character: SourceCharacter::Button,
                press_only: release.is_none(),
                fourteen_bit: false,
            })?;
            // If press-only and we have a value that's neither MAX or MIN, it means we want to a
            // message with this particular value ONLY. In this case it's best to create a raw
            // MIDI message source.
            if let Some(release) = release {
                let release_res = convert_max_short_msg_to_source(MsgConvInput {
                    msg: release,
                    character: SourceCharacter::Button,
                    press_only: false,
                    fourteen_bit: false,
                })?;
                if release_res.source != press_res.source {
                    annotator.warn("Press and release messages differ not just in value but also in type or channel. This is very uncommon and might be a mistake or shortcoming of the widget definition. In general, ReaLearn supports such exotic cases but the CSI-to-ReaLearn conversion not yet. If you really need it, open an issue at GitHub.")
                }
            }
            let mapping = Mapping {
                feedback_enabled: Some(false),
                source: Some(press_res.source),
                glue: {
                    let g = Glue {
                        button_filter: if release.is_some() {
                            None
                        } else {
                            Some(ButtonFilter::PressOnly)
                        },
                        reverse: Some(press_res.reverse_if_button_like),
                        ..Default::default()
                    };
                    Some(g)
                },
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbTwoState { on, off } => {
            let on_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: on,
                character: SourceCharacter::Button,
                press_only: false,
                fourteen_bit: false,
            })?;
            let off_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: off,
                character: SourceCharacter::Button,
                press_only: false,
                fourteen_bit: false,
            })?;
            if off_res.source != on_res.source {
                annotator.warn("On and off messages differ not just in value but also in type or channel. This is very uncommon and might be a mistake or shortcoming of the widget definition. In general, ReaLearn supports such exotic cases but the CSI-to-ReaLearn conversion for this case has not been implemented. If you really need it, open an issue at GitHub.")
            }
            let mapping = Mapping {
                control_enabled: Some(false),
                source: Some(on_res.source),
                glue: {
                    let g = Glue {
                        reverse: Some(on_res.reverse_if_button_like),
                        ..Default::default()
                    };
                    Some(g)
                },
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::Encoder {
            main,
            accelerations,
        } => {
            let character = convert_accelerations_to_character(accelerations, annotator);
            let main_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: main,
                character,
                press_only: false,
                fourteen_bit: false,
            })?;
            let mapping = Mapping {
                feedback_enabled: Some(false),
                source: Some(main_res.source),
                glue: {
                    // TODO-high Respect number of accelerations and set speed max accordingly
                    let g = Glue {
                        ..Default::default()
                    };
                    Some(g)
                },
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbEncoder { max } => {
            let max_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: max,
                character: SourceCharacter::Relative1,
                press_only: false,
                fourteen_bit: false,
            })?;
            let mapping = Mapping {
                control_enabled: Some(false),
                source: Some(max_res.source),
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::Toggle { on } => {
            let on_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: on,
                character: SourceCharacter::Button,
                press_only: false,
                fourteen_bit: false,
            })?;
            let mapping = Mapping {
                feedback_enabled: Some(false),
                source: Some(on_res.source),
                glue: {
                    let g = Glue {
                        reverse: Some(on_res.reverse_if_button_like),
                        ..Default::default()
                    };
                    Some(g)
                },
                // TODO-high Mmh, there's also a separate mapping for that. What's the point of
                //  "Toggle" then? Maybe it's just a duplicate in the X-Touch mst file. Check!
                target: virtual_target(
                    extended_control_element_id(widget_id, "push")?,
                    target_character,
                ),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::Touch { touch, release } => {
            let touch_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: touch,
                character: SourceCharacter::Button,
                press_only: false,
                fourteen_bit: false,
            })?;

            let release_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: release,
                character: SourceCharacter::Button,
                press_only: false,
                fourteen_bit: false,
            })?;
            if release_res.source != touch_res.source {
                annotator.warn("Touch and release messages differ not just in value but also in type or channel. This is very uncommon and might be a mistake or shortcoming of the widget definition. In general, ReaLearn supports such exotic cases but the CSI-to-ReaLearn conversion for this case has not been implemented. If you really need it, open an issue at GitHub.")
            }
            let mapping = Mapping {
                feedback_enabled: Some(false),
                source: Some(touch_res.source),
                glue: {
                    let g = Glue {
                        reverse: Some(touch_res.reverse_if_button_like),
                        ..Default::default()
                    };
                    Some(g)
                },
                target: virtual_target(
                    extended_control_element_id(widget_id, "touch")?,
                    target_character,
                ),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::Fader14Bit { max } => {
            let max_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: max,
                character: SourceCharacter::Range,
                press_only: false,
                fourteen_bit: true,
            })?;
            let mapping = Mapping {
                feedback_enabled: Some(false),
                source: Some(max_res.source),
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbFader14Bit { max } => {
            let max_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: max,
                character: SourceCharacter::Range,
                press_only: false,
                fourteen_bit: true,
            })?;
            let mapping = Mapping {
                control_enabled: Some(false),
                source: Some(max_res.source),
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbMcuVuMeter { index } => {
            let source = Source::MidiRaw(MidiRawSource {
                feedback_behavior: None,
                pattern: Some(format!("D0 [{:04b} dcba]", index)),
                character: Some(SourceCharacter::Range),
            });
            let mapping = Mapping {
                control_enabled: Some(false),
                source: Some(source),
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbMcuTimeDisplay => {
            let source = Source::MackieSevenSegmentDisplay(MackieSevenSegmentDisplaySource {
                scope: Some(MackieSevenSegmentDisplayScope::Tc),
            });
            let mapping = Mapping {
                control_enabled: Some(false),
                source: Some(source),
                target: virtual_target(widget_id.to_owned(), target_character),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbMcuDisplayLower { index } => {
            let mapping = create_mackie_lcd_mapping(base_mapping, widget_id.to_owned(), index, 1);
            vec![mapping]
        }
        Capability::FbMcuDisplayUpper { index } => {
            let mapping = create_mackie_lcd_mapping(base_mapping, widget_id.to_owned(), index, 0);
            vec![mapping]
        }
        Capability::Unknown(_) => {
            annotator.warn("Unknown capability. If this is a valid CSI capability, please create a ReaLearn issue at GitHub.");
            vec![]
        }
    };
    Ok(mappings)
}

fn extended_control_element_id(base: &str, extension: &str) -> CsiResult<String> {
    let res = format!("{}/{}", base, extension);
    if res.len() > MAX_CONTROL_ELEMENT_ID_LENGTH {
        return Err(format!("{} is an invalid control element ID because it's too long. Please shorten the corresponding widget name.", res).into());
    }
    Ok(res)
}

fn virtual_target(id: String, character: VirtualControlElementCharacter) -> Option<Target> {
    let t = VirtualTarget {
        id: VirtualControlElementId::Named(id),
        character: Some(character),
    };
    Some(Target::Virtual(t))
}

fn convert_accelerations_to_character(
    accelerations: Option<Accelerations>,
    annotator: &mut Annotator,
) -> SourceCharacter {
    if let Some(acc) = accelerations {
        // TODO-high
        SourceCharacter::Relative2
    } else {
        SourceCharacter::Relative1
    }
}

const MAX_CONTROL_ELEMENT_ID_LENGTH: usize = 16;

fn convert_widget_name_to_id(name: &str, annotator: &mut Annotator) -> CsiResult<String> {
    let id = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_punctuation())
        .take(MAX_CONTROL_ELEMENT_ID_LENGTH)
        .collect();
    if name.chars().count() > MAX_CONTROL_ELEMENT_ID_LENGTH {
        annotator.info(format!("ReaLearn doesn't allow for virtual control element IDs longer than 16 characters, therefore the widget name \"{}\" was truncated to the ID \"{}\".", name, id));
    }
    Ok(id)
}

struct MsgConvOutput {
    source: Source,
    reverse_if_button_like: bool,
}

struct MsgConvInput {
    msg: RawShortMessage,
    character: SourceCharacter,
    press_only: bool,
    fourteen_bit: bool,
}

impl MsgConvInput {
    fn should_produce_raw_midi_source_7_bit(&self, value: U7) -> bool {
        self.press_only && (1..U7::MAX.get()).contains(&value.get())
    }

    fn should_produce_raw_midi_source_14_bit(&self, value: U14) -> bool {
        self.press_only && (1..U14::MAX.get()).contains(&value.get())
    }
}

fn convert_max_short_msg_to_source(input: MsgConvInput) -> CsiResult<MsgConvOutput> {
    use StructuredShortMessage::*;
    let res = match input.msg.to_structured() {
        NoteOn {
            channel,
            key_number,
            velocity,
        } => {
            if input.should_produce_raw_midi_source_7_bit(velocity) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiNoteVelocity(MidiNoteVelocitySource {
                        feedback_behavior: None,
                        channel: Some(channel.get()),
                        key_number: Some(key_number.get()),
                    }),
                    reverse_if_button_like: velocity == U7::MIN,
                }
            }
        }
        NoteOff {
            channel,
            key_number,
            velocity,
        } => {
            if input.should_produce_raw_midi_source_7_bit(velocity) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiNoteVelocity(MidiNoteVelocitySource {
                        feedback_behavior: None,
                        channel: Some(channel.get()),
                        key_number: Some(key_number.get()),
                    }),
                    reverse_if_button_like: true,
                }
            }
        }
        PolyphonicKeyPressure {
            channel,
            key_number,
            pressure_amount,
        } => {
            if input.should_produce_raw_midi_source_7_bit(pressure_amount) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiPolyphonicKeyPressureAmount(
                        MidiPolyphonicKeyPressureAmountSource {
                            feedback_behavior: None,
                            channel: Some(channel.get()),
                            key_number: Some(key_number.get()),
                        },
                    ),
                    reverse_if_button_like: pressure_amount == U7::MIN,
                }
            }
        }
        ControlChange {
            channel,
            controller_number,
            control_value,
        } => {
            if input.should_produce_raw_midi_source_7_bit(control_value) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiControlChangeValue(MidiControlChangeValueSource {
                        feedback_behavior: None,
                        channel: Some(channel.get()),
                        controller_number: Some(controller_number.get()),
                        character: Some(input.character),
                        fourteen_bit: Some(input.fourteen_bit),
                    }),
                    reverse_if_button_like: control_value == U7::MIN,
                }
            }
        }
        ProgramChange {
            channel,
            program_number,
        } => {
            if input.should_produce_raw_midi_source_7_bit(program_number) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiProgramChangeNumber(MidiProgramChangeNumberSource {
                        feedback_behavior: None,
                        channel: Some(channel.get()),
                    }),
                    reverse_if_button_like: program_number == U7::MIN,
                }
            }
        }
        ChannelPressure {
            channel,
            pressure_amount,
        } => {
            if input.should_produce_raw_midi_source_7_bit(pressure_amount) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiChannelPressureAmount(MidiChannelPressureAmountSource {
                        feedback_behavior: None,
                        channel: Some(channel.get()),
                    }),
                    reverse_if_button_like: pressure_amount == U7::MIN,
                }
            }
        }
        PitchBendChange {
            channel,
            pitch_bend_value,
        } => {
            if input.should_produce_raw_midi_source_14_bit(pitch_bend_value) {
                convert_short_msg_to_raw_midi_source(input)
            } else {
                MsgConvOutput {
                    source: Source::MidiPitchBendChangeValue(MidiPitchBendChangeValueSource {
                        feedback_behavior: None,
                        channel: Some(channel.get()),
                    }),
                    reverse_if_button_like: pitch_bend_value == U14::MIN,
                }
            }
        }
        _ => {
            return Err(format!("Message {:?} not handled in source conversion", input.msg).into())
        }
    };
    Ok(res)
}

fn convert_short_msg_to_raw_midi_source(input: MsgConvInput) -> MsgConvOutput {
    MsgConvOutput {
        source: Source::MidiRaw(MidiRawSource {
            feedback_behavior: None,
            pattern: Some(convert_to_raw_midi_pattern(input.msg)),
            character: Some(input.character),
        }),
        reverse_if_button_like: false,
    }
}

fn convert_to_raw_midi_pattern(msg: RawShortMessage) -> String {
    let (status_byte, data_byte_1, data_byte_2) = msg.to_bytes();
    format!(
        "{:02X} {:02X} {:02X}",
        status_byte,
        data_byte_1.get(),
        data_byte_2.get()
    )
}

fn create_mackie_lcd_mapping(
    base_mapping: Mapping,
    widget_id: String,
    index: u8,
    line: u8,
) -> Mapping {
    let source = Source::MackieLcd(MackieLcdSource {
        channel: Some(index),
        line: Some(line),
    });
    Mapping {
        control_enabled: Some(false),
        source: Some(source),
        target: virtual_target(widget_id, VirtualControlElementCharacter::Multi),
        ..base_mapping
    }
}
