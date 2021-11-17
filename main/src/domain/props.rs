use crate::domain::{
    get_track_color, get_track_name, CompoundChangeEvent, CompoundMappingTarget, ControlContext,
    MainMapping, RealearnTarget, ReaperTarget,
};
use helgoboss_learn::{target_prop_keys, PropValue, Target};
use reaper_high::ChangeEvent;

pub fn prop_is_affected_by(
    key: &str,
    event: CompoundChangeEvent,
    target: &ReaperTarget,
    context: ControlContext,
) -> bool {
    if let Some(target_key) = key.strip_prefix("target.") {
        target_prop_is_affected_by(target_key, event, target, context)
    } else {
        match key {
            _ => false,
        }
    }
}

pub fn target_prop_is_affected_by(
    key: &str,
    event: CompoundChangeEvent,
    target: &ReaperTarget,
    context: ControlContext,
) -> bool {
    match key {
        // These properties always relate to the main target value property.
        target_prop_keys::TEXT_VALUE
        | target_prop_keys::NUMERIC_VALUE
        | target_prop_keys::NORMALIZED_VALUE => target.process_change_event(event, context).0,
        // These properties relate to a secondary target property.
        "track.index" => matches!(
            event,
            CompoundChangeEvent::Reaper(
                ChangeEvent::TrackAdded(_)
                    | ChangeEvent::TrackRemoved(_)
                    | ChangeEvent::TracksReordered(_)
            )
        ),
        "fx.index" => {
            // This could be more specific (taking the track into account) but so what.
            // This doesn't happen that frequently.
            matches!(
                event,
                CompoundChangeEvent::Reaper(
                    ChangeEvent::FxAdded(_)
                        | ChangeEvent::FxRemoved(_)
                        | ChangeEvent::FxReordered(_)
                )
            )
        }
        "track.name" => {
            matches!(event, CompoundChangeEvent::Reaper(ChangeEvent::TrackNameChanged(e)) if Some(&e.track) == target.track())
        }
        "route.name" => {
            // This could be more specific (taking the route partner into account) but so what.
            // Track names are not changed that frequently.
            matches!(
                event,
                CompoundChangeEvent::Reaper(ChangeEvent::TrackNameChanged(_))
            )
        }
        // There are no appropriate REAPER change events for the following properties. Therefore
        // we delegate to the target. Some targets support polling, then it should work definitely.
        "fx.name" | "track.color" | "route.index" => target.process_change_event(event, context).0,
        // These properties are static in nature (change only when target settings change).
        target_prop_keys::NUMERIC_VALUE_UNIT | "type.name" | "type.long_name" => false,
        // Target-specific placeholder. At the moment we should only have target-specific
        // placeholders that are affected by changes of the main target value, so the following
        // is good enough. If this changes in future, we should introduce a similar function
        // in ReaLearn target (one that takes a key).
        _ => target.process_change_event(event, context).0,
    }
}

pub fn get_prop_value(
    key: &str,
    mapping: &MainMapping,
    control_context: ControlContext,
) -> Option<PropValue> {
    if let Some(target_key) = key.strip_prefix("target.") {
        mapping.targets().first().and_then(|t| {
            get_realearn_target_prop_value_with_fallback(t, target_key, control_context)
        })
    } else {
        match key {
            "mapping.name" => {
                let instance_state = control_context.instance_state.borrow();
                let info = instance_state.get_mapping_info(mapping.qualified_id())?;
                Some(PropValue::Text(info.name.clone()))
            }
            _ => None,
        }
    }
}

/// `key` must not have the `target.` prefix anymore when calling this!
pub fn get_realearn_target_prop_value_with_fallback<'a>(
    target: &(impl RealearnTarget + Target<'a, Context = ControlContext<'a>>),
    key: &str,
    context: ControlContext<'a>,
) -> Option<PropValue> {
    target.prop_value(key, context).or_else(|| {
        let res = match key {
            target_prop_keys::TEXT_VALUE => PropValue::Text(target.text_value(context)?),
            target_prop_keys::NUMERIC_VALUE => PropValue::Numeric(target.numeric_value(context)?),
            target_prop_keys::NUMERIC_VALUE_UNIT => {
                PropValue::Text(target.numeric_value_unit(context).to_string())
            }
            target_prop_keys::NORMALIZED_VALUE => {
                PropValue::Normalized(target.current_value(context)?.to_unit_value())
            }
            // At the moment we don't care about a proper maximum value for fractions.
            "type.name" => PropValue::Text(target.reaper_target_type()?.short_name().to_string()),
            "type.long_name" => PropValue::Text(target.reaper_target_type()?.to_string()),
            "track.index" => PropValue::Index(target.track()?.index()?),
            "track.name" => PropValue::Text(get_track_name(target.track()?)),
            "track.color" => PropValue::Color(get_track_color(target.track()?)?),
            "fx.index" => PropValue::Index(target.fx()?.index()),
            "fx.name" => PropValue::Text(target.fx()?.name().into_string()),
            "route.index" => PropValue::Index(target.route()?.index()),
            "route.name" => PropValue::Text(target.route()?.name().into_string()),
            _ => return None,
        };
        Some(res)
    })
}

trait PropDef<T> {
    fn is_affected_by(&self, input: PropIsAffectedByInput<T>) -> bool;
    fn get_value(&self, input: PropGetValueInput<T>) -> Option<PropValue>;
}

struct PropIsAffectedByInput<'a, T> {
    event: CompoundChangeEvent<'a>,
    object: T,
    control_context: ControlContext<'a>,
}

struct PropGetValueInput<'a, T> {
    object: T,
    control_context: ControlContext<'a>,
}

struct MappingAndTarget<'a> {
    mapping: &'a MainMapping,
    target: &'a CompoundMappingTarget,
}

struct MappingNameProp;

impl<'a> PropDef<&'a MainMapping> for MappingNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByInput<&MainMapping>) -> bool {
        // Mapping name changes will result in a full mapping resync anyway.
        false
    }

    fn get_value(&self, input: PropGetValueInput<&MainMapping>) -> Option<PropValue> {
        let instance_state = input.control_context.instance_state.borrow();
        let info = instance_state.get_mapping_info(input.object.qualified_id())?;
        Some(PropValue::Text(info.name.clone()))
    }
}

struct TargetTextValueProp;

impl<'a> PropDef<MappingAndTarget<'a>> for TargetTextValueProp {
    fn is_affected_by(&self, input: PropIsAffectedByInput<MappingAndTarget>) -> bool {
        input
            .object
            .target
            .process_change_event(input.event, input.control_context)
            .0
    }

    fn get_value(&self, input: PropGetValueInput<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            input.object.target.text_value(input.control_context)?,
        ))
    }
}
