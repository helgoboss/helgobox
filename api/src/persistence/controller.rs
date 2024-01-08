use enumset::EnumSetType;
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
    /// Should be unique on a particular machine and ideally globally unique (in potential
    /// merging scenarios).
    pub id: String,
    /// Descriptive name of the controller.
    ///
    /// If one uses multiple controllers of the same kind, this should make clear which
    /// particular controller instance we are talking about.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<ControllerConnection>,
    /// Controller preset to load whenever an auto unit with this controller is created.
    ///
    /// This might be overridden if the controller role main preset uses a virtual control
    /// scheme that is not provided by this controller preset.
    ///
    /// The controller preset is especially important if one of the controller role main
    /// presets is a **reusable main preset**. In that case, a controller preset should be chosen
    /// that supports at least one of the virtual control schemes supported by the main preset,
    /// otherwise the main preset will not have any effect at all!
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_preset: Option<PresetId>,
    #[serde(default)]
    pub roles: ControllerRoles,
}

/// Configuration of different roles that a controller can automatically exercise when
/// triggered by the user.
///
/// The concrete trigger such as "Use this controller for DAW control now" is separate from
/// this configuration. Without a trigger, ReaLearn won't do anything.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ControllerRoles {
    /// Configuration of the "DAW control" role.
    pub daw: Option<ControllerRole>,
    /// Configuration of the "Clip control" role.
    pub clip: Option<ControllerRole>,
}

/// Popular roles a controller can take.
///
/// This sounds like it's similar to *virtual control schemes* but it's clearly different.
/// A virtual control scheme (e.g. DAW, grid or numbered) is about declaring the capabilities of a
/// controller whereas the controller role kind is about a specific employment/usage of a
/// controller ... so it's about mappings in the main compartment!
///
/// The two concepts are orthogonal to each other. Example: The virtual control scheme "grid"
/// suites itself very much to the role "clip control", but it doesn't have to! A "grid" controller
/// can also be used for "DAW" control! Likewise, the virtual control scheme "DAW" suites itself
/// very much to the role "DAW control", but a "DAW" controller can also be used for "clip control".
#[derive(Hash, Debug, Serialize, Deserialize, EnumSetType)]
pub enum ControllerRoleKind {
    /// DAW control.
    Daw,
    /// Clip control.
    Clip,
}

/// Particular configuration of a controller role.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ControllerRole {
    /// The main preset to load for a controller if used in that role.
    ///
    /// It's possible to override the default controller preset defined in the controller, but this
    /// is not something the user does manually. The main preset itself can declare that it depends
    /// on a specific controller preset, in which case that one is used instead of the default
    /// controller preset.
    pub main_preset: Option<PresetId>,
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
    pub fn get(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct MidiOutputPort(u32);

impl MidiOutputPort {
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
pub struct PresetId(String);

impl PresetId {
    pub fn get(&self) -> &str {
        &self.0
    }
}
