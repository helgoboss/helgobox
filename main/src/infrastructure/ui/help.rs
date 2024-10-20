use derive_more::Display;
use helgoboss_learn::ModeParameter;
use include_dir::{include_dir, Dir};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum HelpTopic {
    Mapping(MappingTopic),
    Source(SourceTopic),
    Target(TargetTopic),
    Glue(ModeParameter),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum MappingTopic {
    Bla,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum SourceTopic {
    #[display(fmt = "Source type")]
    Kind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum TargetTopic {
    #[display(fmt = "Target type")]
    Kind,
}

pub struct HelpTopicDetails {
    pub doc_url: String,
    pub control_desc: &'static str,
    pub feedback_desc: &'static str,
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
            control_desc: get_partial_content(&format!("{help_section_id}/{id}-control.txt"))
                .unwrap_or_default(),
            feedback_desc: get_partial_content(&format!("{help_section_id}/{id}-feedback.txt"))
                .unwrap_or_default(),
        };
        Some(details)
    }

    fn qualified_id(&self) -> Option<(HelpSection, &'static str)> {
        let id = match self {
            HelpTopic::Mapping(t) => match t {
                _ => return None,
            },
            HelpTopic::Source(t) => match t {
                _ => return None,
            },
            HelpTopic::Target(t) => match t {
                _ => return None,
            },
            HelpTopic::Glue(mode_parameter) => {
                use ModeParameter::*;
                match mode_parameter {
                    SourceMinMax => (HelpSection::Glue, "source-min-max"),
                    Reverse => (HelpSection::Glue, "reverse"),
                    OutOfRangeBehavior => (HelpSection::Glue, "out-of-range-behavior"),
                    TakeoverMode => (HelpSection::Glue, "takeover-mode"),
                    ControlTransformation => (HelpSection::Glue, "control-transformation"),
                    TargetValueSequence => (HelpSection::Glue, "value-sequence"),
                    TargetMinMax => (HelpSection::Glue, "target-min-max"),
                    FeedbackType | FeedbackTransformation | TextualFeedbackExpression => {
                        (HelpSection::Glue, "feedback-type")
                    }
                    StepSizeMin | StepSizeMax => (HelpSection::Glue, "step-size-min-max"),
                    StepFactorMin | StepFactorMax => (HelpSection::Glue, "speed-min-max"),
                    RelativeFilter => (HelpSection::Glue, "encoder-filter"),
                    Rotate => (HelpSection::Glue, "wrap"),
                    FireMode => (HelpSection::Glue, "fire-mode"),
                    ButtonFilter => (HelpSection::Glue, "button-filter"),
                    MakeAbsolute => (HelpSection::Glue, "make-absolute"),
                    RoundTargetValue => (HelpSection::Glue, "round-target-value"),
                    AbsoluteMode => (HelpSection::Glue, "absolute-mode"),
                    GroupInteraction => (HelpSection::Glue, "group-interaction"),
                    _ => return None,
                }
            }
        };
        Some(id)
    }
}

#[derive(Copy, Clone, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum HelpSection {
    Glue,
}

impl HelpSection {
    fn doc_base_url(&self) -> String {
        match self {
            HelpSection::Glue => format!("{GENERAL_DOC_BASE_URL}/mapping-panel/glue-section.html"),
        }
    }
}

const GENERAL_DOC_BASE_URL: &str = "https://docs.helgoboss.org/realearn/user-interface";
