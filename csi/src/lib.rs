use derive_more::Display;
use helgoboss_midi::{RawShortMessage, ShortMessage, StructuredShortMessage, U14, U7};
use realearn_api::persistence::{
    ApiObject, ButtonFilter, CompartmentContent, Envelope, Glue, Interval, MackieLcdSource,
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
    let widgets = parser::mst_file_content(text)?;
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
                let compartment = CompartmentContent {
                    mappings: Some(mappings),
                    ..Default::default()
                };
                ApiObject::ControllerCompartment(Envelope {
                    version: None,
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
            annotator.with_context(format!("Capability \"{c}\""), |annotator| {
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
        id: Some(format!("{widget_id}-{capability}")),
        name: Some(format!("{widget_name} - {capability}")),
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
            let acc_conv_res = convert_accelerations(accelerations, annotator)?;
            let main_res = convert_max_short_msg_to_source(MsgConvInput {
                msg: main,
                character: acc_conv_res.character,
                press_only: false,
                fourteen_bit: false,
            })?;
            let mapping = Mapping {
                feedback_enabled: Some(false),
                source: Some(main_res.source),
                glue: {
                    let g = Glue {
                        step_factor_interval: Some(acc_conv_res.step_factor_interval),
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
                // TODO-medium Mmh, there's also a separate mapping for that. What's the point of
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
                pattern: Some(format!("D0 [{index:04b} dcba]")),
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
    let res = format!("{base}/{extension}");
    if res.len() > MAX_CONTROL_ELEMENT_ID_LENGTH {
        return Err(format!("{res} is an invalid control element ID because it's too long. Please shorten the corresponding widget name.").into());
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

struct AccelerationConvResult {
    character: SourceCharacter,
    step_factor_interval: Interval<i32>,
}

fn convert_accelerations(
    accelerations: Option<Accelerations>,
    annotator: &mut Annotator,
) -> CsiResult<AccelerationConvResult> {
    let accelerations = if let Some(acc) = accelerations {
        acc
    } else {
        let res = AccelerationConvResult {
            character: SourceCharacter::Relative3,
            step_factor_interval: Interval(1, 1),
        };
        return Ok(res);
    };
    let native_decrements = NativeAcceleration::from_acceleration(accelerations.decrements)
        .map_err(|_| "No acceleration values provided for counter-clockwise encoder movement")?;
    let native_increments = NativeAcceleration::from_acceleration(accelerations.increments)
        .map_err(|_| "No acceleration values provided for clockwise encoder movement")?;
    let neutral_accelerations = neutralize_accelerations(native_decrements, native_increments)?;
    let res = AccelerationConvResult {
        character: neutral_accelerations.character,
        step_factor_interval: Interval(1, neutral_accelerations.max_acceleration()),
    };
    let dec_diff = neutral_accelerations.decrements.diff();
    let inc_diff = neutral_accelerations.increments.diff();
    if dec_diff.is_non_continuous() || inc_diff.is_non_continuous() {
        annotator.warn(
            "Non-continuous acceleration profile detected. Encoder acceleration behavior might be slightly different in ReaLearn.",
        );
    }
    if dec_diff != inc_diff {
        annotator.warn("Clockwise acceleration profile differs from counter-clockwise acceleration profile. In general supported by ReaLearn but not yet supported by the CSI-to-ReaLearn conversion. That means the acceleration behavior might be slightly different in ReaLearn.");
    }
    Ok(res)
}

const MAX_CONTROL_ELEMENT_ID_LENGTH: usize = 16;

fn convert_widget_name_to_id(name: &str, annotator: &mut Annotator) -> CsiResult<String> {
    let id = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_punctuation())
        .take(MAX_CONTROL_ELEMENT_ID_LENGTH)
        .collect();
    if name.chars().count() > MAX_CONTROL_ELEMENT_ID_LENGTH {
        annotator.info(format!("ReaLearn doesn't allow for virtual control element IDs longer than 16 characters, therefore the widget name \"{name}\" was truncated to the ID \"{id}\"."));
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
        extender_index: None,
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

struct NeutralAccelerations {
    character: SourceCharacter,
    /// This should contain values > 1 where each value contains the decrement amount.
    decrements: NeutralAcceleration,
    /// This should contain values > 1 where each value contains the increment amount.
    increments: NeutralAcceleration,
}

impl NeutralAccelerations {
    pub fn max_acceleration(&self) -> i32 {
        std::cmp::max(
            self.decrements.0.iter().max().copied().unwrap_or(0),
            self.increments.0.iter().max().copied().unwrap_or(0),
        )
    }
}

struct NativeAcceleration(Vec<u8>);

impl NativeAcceleration {
    pub fn from_acceleration(acc: Acceleration) -> Result<Self, &'static str> {
        let vec = match acc {
            Acceleration::Sequence(s) => s,
            Acceleration::Range(r) => r.collect(),
        };
        if vec.is_empty() {
            return Err("no acceleration values provided");
        }
        Ok(Self(vec))
    }

    pub fn first(&self) -> u8 {
        *self.0.first().expect("impossible")
    }

    pub fn neutralize(self, crementor: i32) -> NeutralAcceleration {
        let vec = self
            .0
            .into_iter()
            .map(|b| (b as i32 + crementor).abs())
            .collect();
        NeutralAcceleration(vec)
    }
}

struct NeutralAcceleration(Vec<i32>);

impl NeutralAcceleration {
    pub fn diff(&self) -> AccelerationDiff {
        let vec = self
            .0
            .iter()
            .copied()
            .zip(self.0.iter().copied().skip(1))
            .map(|(prev, next)| next - prev)
            .collect();
        AccelerationDiff(vec)
    }
}

#[derive(PartialEq)]
struct AccelerationDiff(Vec<i32>);

impl AccelerationDiff {
    pub fn is_non_continuous(&self) -> bool {
        self.0.iter().any(|d| *d != 1)
    }
}

fn neutralize_accelerations(
    decrements: NativeAcceleration,
    increments: NativeAcceleration,
) -> CsiResult<NeutralAccelerations> {
    let (character, decrementor, incrementor) = match (decrements.first(), increments.first()) {
        (121..=127, 1..=7) => (SourceCharacter::Relative1, -128, 0),
        (57..=63, 65..=71) => (SourceCharacter::Relative2, -64, -64),
        (65..=71, 1..=7) => (SourceCharacter::Relative3, -64, 0),
        _ => return Err("Unsupported relative encoder type".into()),
    };
    let neutralized_acc = NeutralAccelerations {
        character,
        decrements: decrements.neutralize(decrementor),
        increments: increments.neutralize(incrementor),
    };
    Ok(neutralized_acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutralize_accelerations_relative_3() {
        // Given
        let decrements = NativeAcceleration(vec![0x41, 0x42, 0x43]);
        let increments = NativeAcceleration(vec![0x01, 0x02, 0x03]);
        // When
        let neutralized = neutralize_accelerations(decrements, increments).unwrap();
        // Then
        assert_eq!(neutralized.character, SourceCharacter::Relative3);
        assert_eq!(neutralized.decrements.0, vec![1, 2, 3]);
        assert_eq!(neutralized.increments.0, vec![1, 2, 3]);
    }

    #[test]
    fn neutralize_accelerations_relative_1() {
        // Given
        let decrements = NativeAcceleration(vec![0x7f, 0x7e, 0x7c, 0x7a]);
        let increments = NativeAcceleration(vec![0x01, 0x04, 0x07]);
        // When
        let neutralized = neutralize_accelerations(decrements, increments).unwrap();
        // Then
        assert_eq!(neutralized.character, SourceCharacter::Relative1);
        assert_eq!(neutralized.decrements.0, vec![1, 2, 4, 6]);
        assert_eq!(neutralized.increments.0, vec![1, 4, 7]);
    }

    #[test]
    fn neutral_diff() {
        // Given
        let increments = NeutralAcceleration(vec![0x01, 0x04, 0x07]);
        // When
        let diff = increments.diff();
        // Then
        assert_eq!(diff.0, vec![3, 3]);
    }
}
