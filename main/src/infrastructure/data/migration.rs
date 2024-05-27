use semver::Version;

/// The default of this struct is a no-op!
#[derive(Default)]
pub struct MigrationDescriptor {
    /// Invert target interval of mapping when migrating from old version.
    ///
    /// https://github.com/helgoboss/realearn/issues/117
    pub target_interval_transformation_117: bool,
    /// If the FX selector was <Focused> before the "Unit FX" concept was introduced, we
    /// transform it to <Instance> (and set the instance FX by default to <Focused>).
    ///  
    /// https://github.com/helgoboss/realearn/issues/188
    pub fx_selector_transformation_188: bool,
    /// https://github.com/helgoboss/realearn/issues/485
    pub jump_overhaul_485: bool,
    /// https://github.com/helgoboss/realearn/issues/761
    pub action_invocation_swap_761: bool,
    /// https://github.com/helgoboss/realearn/issues/928
    pub fire_mode_after_timeout_release_928: bool,
}

impl MigrationDescriptor {
    pub fn new(preset_version: Option<&Version>) -> MigrationDescriptor {
        MigrationDescriptor {
            // None means it's < 1.12.0-pre18.
            target_interval_transformation_117: preset_version.is_none(),
            fx_selector_transformation_188: if let Some(v) = preset_version {
                let instance_fx_introduction_version = &Version::parse("2.13.0-pre.9").unwrap();
                v < instance_fx_introduction_version
            } else {
                false
            },
            jump_overhaul_485: if let Some(v) = preset_version {
                let jump_overhaul_version = &Version::parse("2.14.0-pre.10").unwrap();
                v < jump_overhaul_version
            } else {
                true
            },
            action_invocation_swap_761: if let Some(v) = preset_version {
                let absolute_7_bit_intro_version = &Version::parse("2.13.0-pre.1").unwrap();
                let swap_fix_version = &Version::parse("2.14.0-pre.11").unwrap();
                v >= absolute_7_bit_intro_version && v < swap_fix_version
            } else {
                false
            },
            fire_mode_after_timeout_release_928: if let Some(v) = preset_version {
                v < &Version::parse("2.14.0-pre.15").unwrap()
            } else {
                true
            },
        }
    }
}
