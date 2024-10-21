use crate::infrastructure::ui::help::SourceTopic::{Category, Learn, Type};
use derive_more::Display;
use helgoboss_learn::ModeParameter;
use include_dir::{include_dir, Dir};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum HelpTopic {
    Concept(ConceptTopic),
    Mapping(MappingTopic),
    Source(SourceTopic),
    Target(TargetTopic),
    Glue(ModeParameter),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum ConceptTopic {
    Mapping,
    Source,
    Target,
    Glue,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum MappingTopic {
    Name,
    Tags,
    #[display(fmt = "Control enabled")]
    ControlEnabled,
    #[display(fmt = "Feedback enabled")]
    FeedbackEnabled,
    Active,
    #[display(fmt = "Feedback mode")]
    FeedbackMode,
    #[display(fmt = "Show in projection")]
    ShowInProjection,
    #[display(fmt = "Advanced settings")]
    AdvancedSettings,
    #[display(fmt = "Find in mapping list")]
    FindInMappingList,
    #[display(fmt = "Beep on success")]
    BeepOnSuccess,
    #[display(fmt = "Go to previous mapping")]
    PreviousMapping,
    #[display(fmt = "Go to next mapping")]
    NextMapping,
    Enabled,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum SourceTopic {
    Learn,
    #[display(fmt = "Source category")]
    Category,
    #[display(fmt = "Source type")]
    Type,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum TargetTopic {
    Learn,
    Menu,
    #[display(fmt = "Target category")]
    Category,
    #[display(fmt = "Target type")]
    Type,
    #[display(fmt = "Current value")]
    CurrentValue,
    #[display(fmt = "Display unit")]
    DisplayUnit,
}

pub struct HelpTopicDetails {
    pub doc_url: String,
    pub description: HelpTopicDescription,
}

pub enum HelpTopicDescription {
    General(&'static str),
    DirectionDependent {
        control_desc: Option<&'static str>,
        feedback_desc: Option<&'static str>,
    },
}

pub static PARTIALS_DIR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../doc/realearn/modules/ROOT/partials");

fn get_partial_content(path: &str) -> Option<&'static str> {
    let file = PARTIALS_DIR.get_file(path)?;
    file.contents_utf8()
}

impl HelpTopic {
    pub fn get_details(&self) -> Option<HelpTopicDetails> {
        let (help_section, id) = self.qualified_id()?;
        let help_section_id: &'static str = help_section.into();
        let details = HelpTopicDetails {
            doc_url: format!("{}#{id}", help_section.doc_base_url()),
            description: if let Some(content) =
                get_partial_content(&format!("{help_section_id}/{id}/general.txt"))
            {
                HelpTopicDescription::General(content)
            } else {
                HelpTopicDescription::DirectionDependent {
                    control_desc: get_partial_content(&format!(
                        "{help_section_id}/{id}/control.txt"
                    )),
                    feedback_desc: get_partial_content(&format!(
                        "{help_section_id}/{id}/feedback.txt"
                    )),
                }
            },
        };
        Some(details)
    }

    fn qualified_id(&self) -> Option<(HelpSection, &'static str)> {
        let qualified_id = match self {
            HelpTopic::Concept(t) => {
                use ConceptTopic::*;
                let id = match t {
                    Mapping => "mapping",
                    Source => "source",
                    Target => "target",
                    Glue => "glue",
                };
                (HelpSection::Concept, id)
            }
            HelpTopic::Mapping(t) => {
                use MappingTopic::*;
                match t {
                    Name => (HelpSection::MappingTop, "name"),
                    Tags => (HelpSection::MappingTop, "tags"),
                    ControlEnabled => (HelpSection::MappingTop, "control-enabled"),
                    FeedbackEnabled => (HelpSection::MappingTop, "feedback-enabled"),
                    Active => (HelpSection::MappingTop, "active"),
                    FeedbackMode => (HelpSection::MappingTop, "feedback-mode"),
                    ShowInProjection => (HelpSection::MappingTop, "show-in-projection"),
                    AdvancedSettings => (HelpSection::MappingTop, "advanced-settings"),
                    FindInMappingList => (HelpSection::MappingTop, "find-in-mapping-list"),
                    BeepOnSuccess => (HelpSection::MappingBottom, "beep-on-success"),
                    PreviousMapping => (HelpSection::MappingBottom, "previous"),
                    NextMapping => (HelpSection::MappingBottom, "next"),
                    Enabled => (HelpSection::MappingBottom, "enabled"),
                }
            }
            HelpTopic::Source(t) => {
                use SourceTopic::*;
                let id = match t {
                    Learn => "learn",
                    Category => "category",
                    Type => "type",
                };
                (HelpSection::Source, id)
            }
            HelpTopic::Target(t) => {
                use TargetTopic::*;
                let id = match t {
                    Learn => "learn",
                    Menu => "menu",
                    Category => "category",
                    Type => "type",
                    CurrentValue => "current-value",
                    DisplayUnit => "display-unit",
                };
                (HelpSection::Target, id)
            }
            HelpTopic::Glue(mode_parameter) => {
                use ModeParameter::*;
                let id = match mode_parameter {
                    SourceMinMax => "source-min-max",
                    Reverse => "reverse",
                    OutOfRangeBehavior => "out-of-range-behavior",
                    TakeoverMode => "takeover-mode",
                    ControlTransformation => "control-transformation",
                    TargetValueSequence => "value-sequence",
                    TargetMinMax => "target-min-max",
                    FeedbackType | FeedbackTransformation | TextualFeedbackExpression => {
                        "feedback-type"
                    }
                    StepSizeMin | StepSizeMax => "step-size-min-max",
                    StepFactorMin | StepFactorMax => "speed-min-max",
                    RelativeFilter => "encoder-filter",
                    Rotate => "wrap",
                    FireMode => "fire-mode",
                    ButtonFilter => "button-filter",
                    MakeAbsolute => "make-absolute",
                    RoundTargetValue => "round-target-value",
                    AbsoluteMode => "absolute-mode",
                    GroupInteraction => "group-interaction",
                    _ => return None,
                };
                (HelpSection::Glue, id)
            }
        };
        Some(qualified_id)
    }
}

#[derive(Copy, Clone, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum HelpSection {
    MappingTop,
    MappingBottom,
    Source,
    Target,
    Glue,
    Concept,
}

impl HelpSection {
    fn doc_base_url(&self) -> String {
        match self {
            HelpSection::Concept => {
                format!("{GENERAL_DOC_BASE_URL}/key-concepts.html")
            }
            HelpSection::MappingTop => {
                format!("{GENERAL_DOC_BASE_URL}/user-interface/mapping-panel/general-section")
            }
            HelpSection::Source => {
                format!("{GENERAL_DOC_BASE_URL}/user-interface/mapping-panel/source-section")
            }
            HelpSection::Target => {
                format!("{GENERAL_DOC_BASE_URL}/user-interface/mapping-panel/target-section")
            }
            HelpSection::Glue => {
                format!("{GENERAL_DOC_BASE_URL}/user-interface/mapping-panel/glue-section")
            }
            HelpSection::MappingBottom => {
                format!("{GENERAL_DOC_BASE_URL}/user-interface/mapping-panel/bottom-section")
            }
        }
    }
}

const GENERAL_DOC_BASE_URL: &str = "https://docs.helgoboss.org/realearn";
