use semver::Version;

pub struct MigrationDescriptor {
    /// https://github.com/helgoboss/realearn/issues/117
    pub target_interval_transformation_117: bool,
}

impl MigrationDescriptor {
    pub fn new(preset_version: Option<&Version>) -> MigrationDescriptor {
        MigrationDescriptor {
            // None means it's < 1.12.0-pre18.
            target_interval_transformation_117: preset_version.is_none(),
        }
    }
}
