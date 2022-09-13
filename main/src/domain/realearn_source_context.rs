use crate::domain::FeedbackOutput;
use helgoboss_learn::SourceContext;
use std::collections::HashMap;

#[derive(Default)]
pub struct RealearnSourceContext {
    source_context_by_feedback_output: HashMap<Option<FeedbackOutput>, SourceContext>,
}

impl RealearnSourceContext {
    pub fn get_source_context(
        &mut self,
        feedback_output: Option<FeedbackOutput>,
    ) -> &mut SourceContext {
        self.source_context_by_feedback_output
            .entry(feedback_output)
            .or_default()
    }
}
