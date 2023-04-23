use crate::base::blocking_lock;
use crate::base::default_util::{deserialize_null_default, is_default};
use crate::domain::pot::{
    BuildInput, BuildOutput, ChangeHint, FilterItem, FilterItemCollections, Filters, MacroParam,
    MacroParamBank, ParamAssignment, PotFilterExcludeList, Preset, Stats,
};
use enum_iterator::IntoEnumIterator;
use enum_map::EnumMap;
use fallible_iterator::FallibleIterator;
use indexmap::IndexSet;
use realearn_api::persistence::PotFilterItemKind;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{Connection, OpenFlags, Row, ToSql};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Eq, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentNksFilterSettings {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub bank: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub sub_bank: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub category: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub sub_category: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mode: Option<String>,
}
