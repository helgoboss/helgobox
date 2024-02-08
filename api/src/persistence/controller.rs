use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ControllerConfig {
    /// All configured controllers.
    #[serde(default)]
    pub controllers: Vec<Controller>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Controller {
    /// ID of the controller.
    ///
    /// Should be unique on a particular machine and ideally globally unique (good for potential
    /// merging scenarios).
    pub id: String,
    /// Descriptive name of the controller.
    ///
    /// If one uses multiple controllers of the same kind, this should make clear which
    /// particular controller instance we are talking about.
    pub name: String,
    /// If not enabled, no auto units will be created for that controller.
    #[serde(default)]
    pub enabled: bool,
    /// Controller color.
    ///
    /// Used e.g. for the control unit rectangle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<ControllerConnection>,
    /// Default controller preset to load whenever an auto unit with this controller is created.
    ///
    /// ReaLearn has mechanisms to automatically identify and load a suitable controller preset
    /// depending on which main preset is loaded. If it has to choose between multiple
    /// candidates and no default controller preset is set, it will prefer a factory controller
    /// preset. If a default controller preset is set and it satisfies the needs of the main preset,
    /// it will use this one instead. It will also use the default controller preset if it can't
    /// automatically identify the correct one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_controller_preset: Option<CompartmentPresetId>,
    /// Default main preset to load whenever an auto unit with this controller is created.
    // TODO-high The plan is to introduce an advanced mode where you don't just set a main preset but can define
    //  a decision table per controller. It's a list of rules. A rule is made from conditions
    //  (fixed number, typed, every condition optional, AND) and effects (fixed number, typed, optional).
    //  The default_main_preset would act as fallback, as last line in the list of rules, which doesn't
    //  define any additional condition.
    //  Possible conditions:
    //  - Playtime clip matrix (if at least one instance is active that has a clip matrix)
    //  - Active pot unit?
    //  Possible effects:
    //  - Main preset (optional)
    //  - Use auto-load in unit
    //  "Use auto-load in unit" uses the already existing global FX-to-preset links to do auto-load within
    //  the unit. FX-to-preset links already is very similar to a decision table. It's going to be a
    //  second global decision table, but a subordinate one, which acts on a single unit (not by adding/removing
    //  units). The main preset defined in the controller rule should act as fallback if none of the
    //  FX-to-preset links was effective. This should be implemented by the existing auto-load. Maybe by
    //  memorizing the preset that was active when "Auto-load depending on instance FX" was active.
    //  FX-to-preset links are not 100% a decision table already. Because their order doesn't matter.
    //  But we can turn it into one by sorting the list according to our current automatic ranking
    //  (a migration step). Making it a decision table would have the nice effect that the user has
    //  much influence and it's immediately clear why something happens, no implicit ranking. The list
    //  of conditions can be easily extended. E.g. we could not just react on what unit FX is
    //  active but also unit track and so on. In any case, we should add a "Controller" condition, so
    //  that one e.g. can load a different main preset depending on which FX is focused AND which controller
    //  is connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_main_preset: Option<CompartmentPresetId>,
}

/// The way a controller is connected to ReaLearn.
///
/// Protocol-specific.
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ControllerConnection {
    Midi(MidiControllerConnection),
    Osc(OscControllerConnection),
}

/// A connection via MIDI.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MidiControllerConnection {
    /// The expected response to a MIDI device inquiry.
    ///
    /// Example: "F0 7E 00 06 02 00 20 6B 02 00 04 02 0E 02 01 01 F7"
    ///
    /// Can be used by ReaLearn to verify whether the device connected to a port is the correct one.   
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_response: Option<String>,
    /// The MIDI input port to which this controller is usually connected on this machine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_port: Option<MidiInputPort>,
    /// The MIDI output port to which this controller is usually connected on this machine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_port: Option<MidiOutputPort>,
}

/// A connection via OSC.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct OscControllerConnection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osc_device_id: Option<OscDeviceId>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MidiInputPort(u32);

impl MidiInputPort {
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn get(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MidiOutputPort(u32);

impl MidiOutputPort {
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn get(&self) -> u32 {
        self.0
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct OscDeviceId(String);

impl OscDeviceId {
    pub fn get(&self) -> &str {
        &self.0
    }
}

/// ID of a controller or main preset (which one depends on the context).
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct CompartmentPresetId(String);

impl CompartmentPresetId {
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    pub fn get(&self) -> &str {
        &self.0
    }
}
