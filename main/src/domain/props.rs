use crate::domain::{
    convert_reaper_color_to_helgoboss_learn, get_fx_name, get_track_name, Backbone,
    CompoundChangeEvent, CompoundMappingTarget, ControlContext, FeedbackResolution, MainMapping,
    RealearnTarget, ReaperTarget, UnresolvedCompoundMappingTarget,
};
use enum_dispatch::enum_dispatch;
use helgoboss_learn::{AbsoluteValue, NumericValue, PropProvider, PropValue, Target};
use helgobox_api::persistence::TrackScope;
use reaper_high::ChangeEvent;
use std::str::FromStr;

/// `None` means that no polling is necessary for feedback because we are notified via events.
pub fn prop_feedback_resolution(
    key: &str,
    mapping: &MainMapping,
    target: &UnresolvedCompoundMappingTarget,
) -> Option<FeedbackResolution> {
    match key.parse::<Props>().ok() {
        Some(props) => props.feedback_resolution(mapping, target),
        None => {
            // Maybe target-specific placeholder. At the moment we should only have target-specific
            // placeholders whose feedback resolution is the same resolution as the one of the
            // main target value, so the following is good enough. If this changes in future, we
            // should introduce a similar function in ReaLearn target (one that takes a key).
            target.feedback_resolution()
        }
    }
}

pub fn prop_is_affected_by(
    key: &str,
    event: CompoundChangeEvent,
    mapping: &MainMapping,
    target: &ReaperTarget,
    control_context: ControlContext,
) -> bool {
    match key.parse::<Props>().ok() {
        Some(props) => {
            // TODO-medium Not very consequent? Here we take the first target and for
            //  target-specific placeholders the given one. A bit hard to change though. Let's see.
            props.is_affected_by(event, mapping, mapping.targets().first(), control_context)
        }
        None => {
            // Maybe target-specific placeholder. At the moment we should only have target-specific
            // placeholders that are affected by changes of the main target value, so the following
            // is good enough. If this changes in future, we should introduce a similar function
            // in ReaLearn target (one that takes a key).
            if key.starts_with("target.") {
                target.process_change_event(event, control_context).0
            } else {
                false
            }
        }
    }
}

pub struct MappingPropProvider<'a> {
    mapping: &'a MainMapping,
    context: ControlContext<'a>,
}

impl<'a> MappingPropProvider<'a> {
    pub fn new(mapping: &'a MainMapping, context: ControlContext<'a>) -> Self {
        Self { mapping, context }
    }
}

impl PropProvider for MappingPropProvider<'_> {
    fn get_prop_value(&self, key: &str) -> Option<PropValue> {
        match key.parse::<Props>().ok() {
            Some(props) => {
                props.get_value(self.mapping, self.mapping.targets().first(), self.context)
            }
            None => {
                let target = self.mapping.targets().first()?;
                if key == "y" {
                    let y = target.current_value(self.context)?;
                    Some(PropValue::Normalized(y.to_unit_value()))
                } else if let Some(key) = key.strip_prefix("target.") {
                    target.prop_value(key, self.context)
                } else {
                    None
                }
            }
        }
    }
}

enum Props {
    Global(GlobalProps),
    Mapping(MappingProps),
    Target(TargetProps),
}

impl Props {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    pub fn feedback_resolution(
        &self,
        mapping: &MainMapping,
        target: &UnresolvedCompoundMappingTarget,
    ) -> Option<FeedbackResolution> {
        match self {
            Props::Global(p) => {
                let args = PropFeedbackResolutionArgs { object: () };
                p.feedback_resolution(args)
            }
            Props::Mapping(p) => {
                let args = PropFeedbackResolutionArgs { object: mapping };
                p.feedback_resolution(args)
            }
            Props::Target(p) => {
                let args = PropFeedbackResolutionArgs {
                    object: MappingAndUnresolvedTarget { mapping, target },
                };
                p.feedback_resolution(args)
            }
        }
    }

    /// Returns whether the value of this property could be affected by the given change event.
    pub fn is_affected_by(
        &self,
        event: CompoundChangeEvent,
        mapping: &MainMapping,
        target: Option<&CompoundMappingTarget>,
        control_context: ControlContext,
    ) -> bool {
        match self {
            Props::Global(p) => {
                let args = PropIsAffectedByArgs {
                    event,
                    object: (),
                    control_context,
                };
                p.is_affected_by(args)
            }
            Props::Mapping(p) => {
                let args = PropIsAffectedByArgs {
                    event,
                    object: mapping,
                    control_context,
                };
                p.is_affected_by(args)
            }
            Props::Target(p) => target
                .map(|target| {
                    let args = PropIsAffectedByArgs {
                        event,
                        object: MappingAndTarget { mapping, target },
                        control_context,
                    };
                    p.is_affected_by(args)
                })
                .unwrap_or(false),
        }
    }

    /// Returns the current value of this property.
    pub fn get_value(
        &self,
        mapping: &MainMapping,
        target: Option<&CompoundMappingTarget>,
        control_context: ControlContext,
    ) -> Option<PropValue> {
        match self {
            Props::Global(p) => {
                let args = PropGetValueArgs {
                    object: (),
                    control_context,
                };
                p.get_value(args)
            }
            Props::Mapping(p) => {
                let args = PropGetValueArgs {
                    object: mapping,
                    control_context,
                };
                p.get_value(args)
            }
            Props::Target(p) => target.and_then(|target| {
                let args = PropGetValueArgs {
                    object: MappingAndTarget { mapping, target },
                    control_context,
                };
                p.get_value(args)
            }),
        }
    }
}

impl FromStr for Props {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<GlobalProps>()
            .map(Props::Global)
            .or_else(|_| s.parse::<MappingProps>().map(Props::Mapping))
            .or_else(|_| s.parse::<TargetProps>().map(Props::Target))
    }
}

#[enum_dispatch]
#[derive(strum::EnumString)]
enum GlobalProps {
    #[strum(serialize = "global.realearn.time")]
    GlobalRealearnTime(GlobalRealearnTimeProp),
}

#[enum_dispatch]
#[derive(strum::EnumString)]
enum MappingProps {
    #[strum(serialize = "mapping.name")]
    Name(MappingNameProp),
}

#[enum_dispatch]
#[derive(strum::EnumString)]
enum TargetProps {
    #[strum(serialize = "target.type.name")]
    TargetTypeName(TargetTypeNameProp),
    #[strum(serialize = "target.type.long_name")]
    TargetTypeLongName(TargetTypeLongNameProp),
    #[strum(serialize = "target.available")]
    TargetAvailable(TargetAvailableProp),
    #[strum(serialize = "target.text_value")]
    TextValue(TargetTextValueProp),
    #[strum(serialize = "target.discrete_value")]
    DiscreteValue(TargetDiscreteValueProp),
    #[strum(serialize = "target.discrete_value_count")]
    DiscreteValueCount(TargetDiscreteValueCountProp),
    #[strum(serialize = "target.numeric_value")]
    NumericValue(TargetNumericValueProp),
    #[strum(serialize = "target.numeric_value.unit")]
    NumericValueUnit(TargetNumericValueUnitProp),
    #[strum(serialize = "target.normalized_value")]
    NormalizedValue(TargetNormalizedValueProp),
    #[strum(serialize = "target.track.index")]
    TrackIndex(TargetTrackIndexProp),
    #[strum(serialize = "target.track.name")]
    TrackName(TargetTrackNameProp),
    #[strum(serialize = "target.track.color")]
    TrackColor(TargetTrackColorProp),
    #[strum(serialize = "target.fx.index")]
    FxIndex(TargetFxIndexProp),
    #[strum(serialize = "target.fx.name")]
    FxName(TargetFxNameProp),
    #[strum(serialize = "target.route.index")]
    RouteIndex(TargetRouteIndexProp),
    #[strum(serialize = "target.route.name")]
    RouteName(TargetRouteNameProp),
    #[strum(serialize = "target.slot.color")]
    PlaytimeSlotColor(TargetPlaytimeSlotColorProp),
}

#[enum_dispatch(GlobalProps)]
trait GlobalProp {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<()>,
    ) -> Option<FeedbackResolution> {
        let _ = args;
        None
    }

    /// Returns whether the value of this property could be affected by the given change event.
    fn is_affected_by(&self, args: PropIsAffectedByArgs<()>) -> bool;

    /// Returns the current value of this property.
    fn get_value(&self, args: PropGetValueArgs<()>) -> Option<PropValue>;
}

#[enum_dispatch(MappingProps)]
trait MappingProp {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<&MainMapping>,
    ) -> Option<FeedbackResolution> {
        let _ = args;
        None
    }

    /// Returns whether the value of this property could be affected by the given change event.
    fn is_affected_by(&self, args: PropIsAffectedByArgs<&MainMapping>) -> bool;

    /// Returns the current value of this property.
    fn get_value(&self, args: PropGetValueArgs<&MainMapping>) -> Option<PropValue>;
}

#[enum_dispatch(TargetProps)]
trait TargetProp {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        let _ = args;
        None
    }

    /// Returns whether the value of this property could be affected by the given change event.
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Many target props change whenever the main target value changes. So this is the default.
        args.object
            .target
            .process_change_event(args.event, args.control_context)
            .0
    }

    /// Returns the current value of this property.
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue>;
}

#[allow(dead_code)]
struct MappingAndTarget<'a> {
    pub mapping: &'a MainMapping,
    pub target: &'a CompoundMappingTarget,
}

#[allow(dead_code)]
struct MappingAndUnresolvedTarget<'a> {
    pub mapping: &'a MainMapping,
    pub target: &'a UnresolvedCompoundMappingTarget,
}

#[allow(dead_code)]
struct PropFeedbackResolutionArgs<T> {
    object: T,
}

struct PropIsAffectedByArgs<'a, T> {
    event: CompoundChangeEvent<'a>,
    object: T,
    control_context: ControlContext<'a>,
}

struct PropGetValueArgs<'a, T> {
    object: T,
    control_context: ControlContext<'a>,
}

#[derive(Default)]
struct GlobalRealearnTimeProp;

impl GlobalProp for GlobalRealearnTimeProp {
    fn feedback_resolution(&self, _: PropFeedbackResolutionArgs<()>) -> Option<FeedbackResolution> {
        Some(FeedbackResolution::High)
    }

    fn is_affected_by(&self, _: PropIsAffectedByArgs<()>) -> bool {
        false
    }

    fn get_value(&self, _: PropGetValueArgs<()>) -> Option<PropValue> {
        Some(PropValue::DurationInMillis(
            Backbone::get().duration_since_time_of_start().as_millis() as _,
        ))
    }
}

#[derive(Default)]
struct MappingNameProp;

impl MappingProp for MappingNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<&MainMapping>) -> bool {
        // Mapping name changes will result in a full mapping resync anyway.
        false
    }

    fn get_value(&self, input: PropGetValueArgs<&MainMapping>) -> Option<PropValue> {
        let instance_state = input.control_context.unit.borrow();
        let info = instance_state.get_mapping_info(input.object.qualified_id())?;
        Some(PropValue::Text(info.name.clone().into()))
    }
}

#[derive(Default)]
struct TargetTextValueProp;

impl TargetProp for TargetTextValueProp {
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        args.object.target.feedback_resolution()
    }

    fn get_value(&self, input: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            input.object.target.text_value(input.control_context)?,
        ))
    }
}

#[derive(Default)]
struct TargetDiscreteValueCountProp;

impl TargetProp for TargetDiscreteValueCountProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        let discrete_count = args
            .object
            .target
            .control_type_and_character(args.control_context)
            .0
            .discrete_count()?;
        Some(PropValue::Numeric(NumericValue::Discrete(
            discrete_count as _,
        )))
    }
}

#[derive(Default)]
struct TargetDiscreteValueProp;

impl TargetProp for TargetDiscreteValueProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        let i = match args.object.target.current_value(args.control_context)? {
            AbsoluteValue::Continuous(uv) => {
                let step_size = args
                    .object
                    .target
                    .control_type_and_character(args.control_context)
                    .0
                    .step_size()?;
                (uv.get() / step_size.get()).round() as u32
            }
            AbsoluteValue::Discrete(d) => d.actual(),
        };
        Some(PropValue::Index(i))
    }
}

#[derive(Default)]
struct TargetNumericValueProp;

impl TargetProp for TargetNumericValueProp {
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        args.object.target.feedback_resolution()
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Numeric(
            args.object.target.numeric_value(args.control_context)?,
        ))
    }
}

#[derive(Default)]
struct TargetNormalizedValueProp;

impl TargetProp for TargetNormalizedValueProp {
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        args.object.target.feedback_resolution()
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Normalized(
            args.object
                .target
                .current_value(args.control_context)?
                .to_unit_value(),
        ))
    }
}

#[derive(Default)]
struct TargetTrackIndexProp;

impl TargetProp for TargetTrackIndexProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        matches!(
            args.event,
            CompoundChangeEvent::Reaper(
                ChangeEvent::TrackAdded(_)
                    | ChangeEvent::TrackRemoved(_)
                    | ChangeEvent::TracksReordered(_)
            )
        )
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Index(args.object.target.track()?.index()?))
    }
}

#[derive(Default)]
struct TargetFxIndexProp;

impl TargetProp for TargetFxIndexProp {
    fn feedback_resolution(
        &self,
        _: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        // This is unfortunately necessary because it's possible that the targeted FX is on the
        // monitoring FX chain. This chain doesn't support notifications, so `is_affected_by`
        // won't work.
        Some(FeedbackResolution::High)
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Index(args.object.target.fx()?.index()))
    }
}

#[derive(Default)]
struct TargetTrackNameProp;

impl TargetProp for TargetTrackNameProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // This could be more specific (taking the track into account) but so what.
        // This doesn't happen that frequently.
        matches!(args.event, CompoundChangeEvent::Reaper(ChangeEvent::TrackNameChanged(e)) if Some(&e.track) == args.object.target.track())
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        let name = get_track_name(args.object.target.track()?, TrackScope::AllTracks);
        Some(PropValue::Text(name.into()))
    }
}

#[derive(Default)]
struct TargetNumericValueUnitProp;

impl TargetProp for TargetNumericValueUnitProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Static in nature (change only when target settings change).
        false
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object
                .target
                .numeric_value_unit(args.control_context)
                .into(),
        ))
    }
}

#[derive(Default)]
struct TargetTypeNameProp;

impl TargetProp for TargetTypeNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Static in nature (change only when target settings change).
        false
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object.target.reaper_target_type()?.short_name().into(),
        ))
    }
}

#[derive(Default)]
struct TargetTypeLongNameProp;

impl TargetProp for TargetTypeLongNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Static in nature (change only when target settings change).
        false
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object.target.reaper_target_type()?.to_string().into(),
        ))
    }
}

#[derive(Default)]
struct TargetAvailableProp;

impl TargetProp for TargetAvailableProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        let is_available = args.object.target.is_available(args.control_context);
        Some(PropValue::Boolean(is_available))
    }
}

#[derive(Default)]
struct TargetTrackColorProp;

impl TargetProp for TargetTrackColorProp {
    fn feedback_resolution(
        &self,
        _: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        // There are no appropriate change events for this property so we fall back to polling.
        Some(FeedbackResolution::High)
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        let color =
            convert_reaper_color_to_helgoboss_learn(args.object.target.track()?.custom_color()?);
        Some(PropValue::Color(color))
    }
}

#[derive(Default)]
struct TargetPlaytimeSlotColorProp;

impl TargetProp for TargetPlaytimeSlotColorProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = args;
            false
        }
        #[cfg(feature = "playtime")]
        {
            use playtime_clip_engine::base::*;
            use playtime_clip_engine::rt::*;
            matches!(
                args.event,
                CompoundChangeEvent::ClipMatrix(
                    ClipMatrixEvent::TrackChanged(_)
                        | ClipMatrixEvent::ClipChanged(QualifiedClipChangeEvent {
                            event: ClipChangeEvent::Content | ClipChangeEvent::Everything,
                            ..
                        })
                )
            )
        }
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = args;
            None
        }
        #[cfg(feature = "playtime")]
        {
            let slot_address = args.object.target.clip_slot_address()?;
            let instance = args.control_context.instance.borrow();
            let matrix = instance.clip_matrix()?;
            let reaper_color = matrix.resolve_slot_color(slot_address)?;
            let final_color = convert_reaper_color_to_helgoboss_learn(reaper_color);
            Some(PropValue::Color(final_color))
        }
    }
}

#[derive(Default)]
struct TargetFxNameProp;

// There are no appropriate REAPER change events for this property.
impl TargetProp for TargetFxNameProp {
    fn feedback_resolution(
        &self,
        _: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        // This is unfortunately necessary because it's possible that the targeted FX is on the
        // monitoring FX chain. This chain doesn't support notifications, so `is_affected_by`
        // won't work.
        Some(FeedbackResolution::High)
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        let name = get_fx_name(args.object.target.fx()?).into();
        Some(PropValue::Text(name))
    }
}

#[derive(Default)]
struct TargetRouteIndexProp;

// There are no appropriate REAPER change events for this property.
impl TargetProp for TargetRouteIndexProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Index(args.object.target.route()?.index()))
    }
}

#[derive(Default)]
struct TargetRouteNameProp;

impl TargetProp for TargetRouteNameProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // This could be more specific (taking the route partner into account) but so what.
        // Track names are not changed that frequently.
        matches!(
            args.event,
            CompoundChangeEvent::Reaper(ChangeEvent::TrackNameChanged(_))
        )
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object.target.route()?.name().into_string().into(),
        ))
    }
}
