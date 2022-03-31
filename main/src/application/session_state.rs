use crate::domain::{
    MappingCompartment, ParameterSetting, ParameterSettingArray, Parameters,
    COMPARTMENT_PARAMETER_COUNT, PLUGIN_PARAMETER_COUNT,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type SharedSessionState = Rc<RefCell<SessionState>>;

/// Holds state of the session that should be shared without requiring to borrow the session.
///
/// Used to prevent a bunch of borrow errors if session queries info (e.g. parameter names) of
/// targets that relate to the own ReaLearn instance (<This>) - which causes reentrancy. We could
/// avoid this reentrancy by avoid going the "ReaLearn => REAPER => ReaLearn FX" detour if we detect
/// that we are actually talking to ourselves. But that would be ugly because it introduces special
/// handling, even in more than just one place. We should be able to "talk to ourselves" from
/// outside, because ReaLearn's philosophy is that it considers itself as yet another normal FX, so
/// special handling would be against its philosophy.
#[derive(Debug)]
pub struct SessionState {
    parameter_settings: ParameterSettingArray,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            parameter_settings: [Default::default(); PLUGIN_PARAMETER_COUNT as usize],
        }
    }
}

impl SessionState {
    pub fn get_parameter_setting(
        &self,
        compartment: MappingCompartment,
        index: u32,
    ) -> &ParameterSetting {
        &self.compartment_parameter_settings(compartment)[index as usize]
    }

    pub fn compartment_parameters(
        &self,
        compartment: MappingCompartment,
        all_values: &[f32],
    ) -> Parameters {
        self.all_parameters(all_values)
            .slice(compartment.param_range())
    }

    fn all_parameters(&self, all_values: &[f32]) -> Parameters {
        Parameters::new(all_values, &self.parameter_settings)
    }

    pub fn non_default_parameter_settings_by_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> HashMap<u32, ParameterSetting> {
        self.compartment_parameter_settings(compartment)
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_default())
            .map(|(i, s)| (i as u32, s.clone()))
            .collect()
    }

    pub fn get_qualified_parameter_name(
        &self,
        compartment: MappingCompartment,
        rel_index: u32,
    ) -> String {
        let name = self.get_parameter_name(compartment, rel_index);
        let compartment_label = match compartment {
            MappingCompartment::ControllerMappings => "Ctrl",
            MappingCompartment::MainMappings => "Main",
        };
        format!("{} p{}: {}", compartment_label, rel_index + 1, name)
    }

    pub fn get_parameter_name(&self, compartment: MappingCompartment, rel_index: u32) -> String {
        let setting = &self.compartment_parameter_settings(compartment)[rel_index as usize];
        if setting.name.is_empty() {
            format!("Param {}", rel_index + 1)
        } else {
            setting.name.clone()
        }
    }

    pub fn set_compartment_parameter_settings_without_notification(
        &mut self,
        compartment: MappingCompartment,
        parameter_settings: Vec<ParameterSetting>,
    ) {
        self.set_compartment_parameter_settings_without_notification_from_iter(
            compartment,
            parameter_settings.into_iter(),
        )
    }

    fn set_compartment_parameter_settings_without_notification_from_iter(
        &mut self,
        compartment: MappingCompartment,
        settings: impl Iterator<Item = ParameterSetting>,
    ) {
        let compartment_settings = self.compartment_parameter_settings_mut(compartment);
        for (i, s) in settings.enumerate() {
            compartment_settings[i] = s;
        }
    }

    pub fn set_compartment_parameter_settings_without_notification_from_indexed_iter(
        &mut self,
        compartment: MappingCompartment,
        settings: impl Iterator<Item = (u32, ParameterSetting)>,
    ) {
        let compartment_settings = self.compartment_parameter_settings_mut(compartment);
        for (i, s) in settings {
            compartment_settings[i as usize] = s;
        }
    }

    pub fn set_parameter_settings_from_non_default(
        &mut self,
        compartment: MappingCompartment,
        parameter_settings: HashMap<u32, ParameterSetting>,
    ) {
        let mut settings = empty_parameter_settings();
        for (i, s) in parameter_settings {
            settings[i as usize] = s;
        }
        self.set_compartment_parameter_settings_without_notification_from_iter(
            compartment,
            settings.into_iter(),
        );
    }

    pub fn find_parameter_setting_by_key(
        &self,
        compartment: MappingCompartment,
        key: &str,
    ) -> Option<(u32, &ParameterSetting)> {
        self.compartment_parameter_settings(compartment)
            .iter()
            .enumerate()
            .find(|(_, s)| s.key.as_ref().map(|k| k == key).unwrap_or(false))
            .map(|(i, s)| (i as u32, s))
    }

    fn compartment_parameter_settings(
        &self,
        compartment: MappingCompartment,
    ) -> &[ParameterSetting] {
        &self.parameter_settings[compartment.param_range_for_indexing()]
    }

    fn compartment_parameter_settings_mut(
        &self,
        compartment: MappingCompartment,
    ) -> &mut [ParameterSetting] {
        &mut self.parameter_settings[compartment.param_range_for_indexing()]
    }
}

pub fn empty_parameter_settings() -> Vec<ParameterSetting> {
    vec![Default::default(); COMPARTMENT_PARAMETER_COUNT as usize]
}
