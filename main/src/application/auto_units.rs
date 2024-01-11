use crate::domain::{ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput};

/// Data about an automatically loaded unit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AutoUnitData {
    pub controller_id: String,
    pub controller_palette_color: Option<u32>,
    pub input: Option<DeviceControlInput>,
    pub output: Option<DeviceFeedbackOutput>,
    pub controller_preset_usage: Option<ControllerPresetUsage>,
    pub main_preset_id: String,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ControllerPresetUsage {
    pub controller_preset_id: String,
    /// `None` means that the default controller preset has been taken as a last resort even
    /// though it couldn't be verified that it's suitable.
    pub main_preset_suitability: Option<MainPresetSuitability>,
    /// `None` means that the default controller preset has been taken as a last resort even
    /// though it couldn't be verified that it's suitable.
    pub controller_suitability: Option<ControllerSuitability>,
}

/// Suitability of a controller preset for a main preset.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct MainPresetSuitability(u8);

impl MainPresetSuitability {
    pub fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub fn get(&self) -> u8 {
        self.0
    }

    pub fn is_generally_suitable(&self) -> bool {
        self.0 > 0
    }
}

/// Suitability of a controller preset for a connected controller.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub enum ControllerSuitability {
    NotSuitable = 0,
    MaybeSuitable = 1,
    Suitable = 2,
}

impl AutoUnitData {
    pub fn control_input(&self) -> ControlInput {
        self.input
            .map(ControlInput::from_device_input)
            .unwrap_or_default()
    }

    pub fn feedback_output(&self) -> Option<FeedbackOutput> {
        self.output.map(FeedbackOutput::from_device_output)
    }
}
