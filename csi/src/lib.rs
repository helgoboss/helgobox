use helgoboss_midi::{RawShortMessage, ShortMessage, StructuredShortMessage};
use realearn_api::schema::{
    ApiObject, Compartment, Envelope, Mapping, MidiNoteVelocitySource, Source, Target,
    VirtualControlElementCharacter, VirtualControlElementId, VirtualTarget,
};
use std::error::Error;

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

pub struct AnnotatedResult<T> {
    pub value: T,
    pub annotations: Vec<String>,
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
        let mut annotations = vec![];
        use CsiObject::*;
        let api_object = match self {
            Widgets(widgets) => {
                let compartment = Compartment {
                    mappings: {
                        let mappings = widgets
                            .into_iter()
                            .flat_map(|w| match convert_widget_to_mappings(w) {
                                Ok(r) => {
                                    annotations.extend(r.annotations);
                                    r.value
                                }
                                Err(e) => {
                                    annotations.push(e.to_string());
                                    vec![]
                                }
                            })
                            .collect();
                        Some(mappings)
                    },
                    ..Default::default()
                };
                ApiObject::ControllerCompartment(Envelope {
                    value: Box::new(compartment),
                })
            }
        };
        let res = AnnotatedResult {
            value: api_object,
            annotations,
        };
        Ok(res)
    }
}

fn convert_widget_to_mappings(widget: Widget) -> CsiResult<AnnotatedResult<Vec<Mapping>>> {
    let mut annotations = vec![];
    let widget_name = widget.name;
    let mappings = widget
        .capabilities
        .into_iter()
        .flat_map(|c| match convert_capability_to_mappings(&widget_name, c) {
            Ok(r) => {
                annotations.extend(r.annotations);
                r.value
            }
            Err(msg) => {
                annotations.push(msg.to_string());
                vec![]
            }
        })
        .collect();
    let res = AnnotatedResult {
        value: mappings,
        annotations,
    };
    Ok(res)
}

fn convert_capability_to_mappings(
    widget_name: &str,
    capability: Capability,
) -> CsiResult<AnnotatedResult<Vec<Mapping>>> {
    let widget_id = convert_widget_name_to_id(widget_name)?;
    let mut annotations = vec![];
    let base_mapping = Mapping {
        name: Some(widget_name.to_owned()),
        target: {
            let t = VirtualTarget {
                id: VirtualControlElementId::Named(widget_id.clone()),
                character: {
                    let ch = if capability.is_virtual_button() {
                        VirtualControlElementCharacter::Button
                    } else {
                        VirtualControlElementCharacter::Multi
                    };
                    Some(ch)
                },
            };
            Some(Target::Virtual(t))
        },
        key: None,
        tags: None,
        group: None,
        visible_in_projection: None,
        enabled: None,
        control_enabled: None,
        feedback_enabled: None,
        activation_condition: None,
        on_activate: None,
        on_deactivate: None,
        source: None,
        glue: None,
        unprocessed: None,
    };
    let mappings = match capability {
        Capability::Press { press, release } => {
            let mapping = Mapping {
                source: Some(convert_on_msg_to_source(press)?),
                ..base_mapping
            };
            vec![mapping]
        }
        Capability::FbTwoState { .. }
        | Capability::Encoder { .. }
        | Capability::FbEncoder { .. }
        | Capability::Toggle { .. }
        | Capability::Fader14Bit { .. }
        | Capability::FbFader14Bit { .. }
        | Capability::Touch { .. }
        | Capability::FbMcuDisplayLower { .. }
        | Capability::FbMcuDisplayUpper { .. }
        | Capability::FbMcuTimeDisplay
        | Capability::FbMcuVuMeter { .. } => vec![],
        Capability::Unknown(expression) => {
            let msg = format!(
                "Encountered unknown capability in widget {}: {}",
                widget_name, expression
            );
            annotations.push(msg);
            vec![]
        }
    };
    let res = AnnotatedResult {
        value: mappings,
        annotations,
    };
    Ok(res)
}

fn convert_widget_name_to_id(name: &str) -> CsiResult<String> {
    // TODO-medium Potential issue: If too long, ReaLearn will crop the ID. This might lead to
    //  incorrect duplicates.
    let id = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_punctuation())
        .collect();
    Ok(id)
}

fn convert_on_msg_to_source(msg: RawShortMessage) -> CsiResult<Source> {
    use StructuredShortMessage::*;
    let source = match msg.to_structured() {
        NoteOn {
            channel,
            key_number,
            velocity,
        }
        | NoteOff {
            channel,
            key_number,
            velocity,
        } => Source::MidiNoteVelocity(MidiNoteVelocitySource {
            feedback_behavior: None,
            channel: Some(channel.get()),
            key_number: Some(key_number.get()),
        }),
        PolyphonicKeyPressure { .. }
        | ControlChange { .. }
        | ProgramChange { .. }
        | ChannelPressure { .. }
        | PitchBendChange { .. } => Source::NoneSource,
        _ => return Err(format!("Message {:?} not handled in source conversion", msg).into()),
    };
    Ok(source)
}
