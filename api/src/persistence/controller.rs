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
    /// Optional controller preset to use with this controller.
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

/// Particular configuration of a controller role.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ControllerRole {
    /// The main preset to load for a controller if used in that role.
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
