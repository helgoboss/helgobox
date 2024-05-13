use crate::persistence::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Complete content of a ReaLearn compartment, including mappings, groups, parameters etc.
#[derive(Default, Serialize, Deserialize)]
pub struct Compartment {
    /// Settings of the default group in this compartment.
    ///
    /// Group fields `id` and `name` will be ignored for the default group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_group: Option<Group>,
    /// All parameters in this compartment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,
    /// All mapping groups in this compartment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<Group>>,
    /// All mappings in this compartment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Vec<Mapping>>,
    /// Lua code that will be compiled only once and can then be reused in various Lua scripts within mappings.
    ///
    /// This code should return a value. This value will then be made available to the scripts. How exactly, depends
    /// on the particular kind of script. In most cases, you want to return a table that contains functions, variables
    /// and other stuff that you want to make available in your scripts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_lua: Option<String>,
    /// Arbitrarily formed data in this compartment.
    ///
    /// The first level is a key-value map where a key represents a sort of namespace. E.g. data that's relevant
    /// for the application ReaLearn Companion has key "companion" and data relevant for the application Playtime has
    /// the key "playtime". Everything nested below is application-specific.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<HashMap<String, serde_json::Value>>,
    /// Can contain text notes, e.g. a helpful description of this compartment, instructions etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub unknown_props: Option<BTreeMap<String, serde_json::Value>>,
}
