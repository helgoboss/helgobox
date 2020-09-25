use uuid::Uuid;

pub fn prompt_for(caption: &str) -> Result<String, &'static str> {
    // TODO-high Implement via GetUserInputs
    Ok(Uuid::new_v4().to_string())
}
