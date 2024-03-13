use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};

use realearn_api::persistence::Axis;

#[derive(Debug)]
pub struct UnresolvedPlaytimeControlUnitScrollTarget {
    pub axis: Axis,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeControlUnitScrollTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = PlaytimeControlUnitScrollTarget { axis: self.axis };
        Ok(vec![ReaperTarget::PlaytimeControlUnitScroll(target)])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeControlUnitScrollTarget {
    axis: Axis,
}

pub const PLAYTIME_CONTROL_UNIT_SCROLL_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Control unit scroll",
    short_name: "Playtime scroll",
    supports_axis: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeControlUnitScrollTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeControlUnitScrollTarget {}
    impl<'a> Target<'a> for PlaytimeControlUnitScrollTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        convert_count_to_step_size, convert_unit_to_discrete_value, CompoundChangeEvent,
        ControlContext, HitResponse, MappingControlContext, PlaytimeControlUnitScrollTarget,
        RealearnTarget, ReaperTargetType, TargetCharacter, UnitEvent,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target};
    use playtime_api::persistence::SlotAddress;
    #[cfg(feature = "playtime")]
    use playtime_clip_engine::base::ClipMatrixEvent;
    use realearn_api::persistence::Axis;

    impl PlaytimeControlUnitScrollTarget {
        fn value_count(&self, context: ControlContext) -> u32 {
            let total_count = context
                .instance()
                .borrow()
                .clip_matrix()
                .map(|m| match self.axis {
                    Axis::X => m.column_count(),
                    Axis::Y => m.row_count(),
                } as u32)
                .unwrap_or(0);
            let unit = context.unit.borrow();
            let control_unit_count = match self.axis {
                Axis::X => unit.control_unit_column_count(),
                Axis::Y => unit.control_unit_row_count(),
            };
            (total_count + 1).saturating_sub(control_unit_count)
        }

        fn calculate_value(
            &self,
            context: ControlContext,
            slot_address: SlotAddress,
        ) -> Option<AbsoluteValue> {
            let index = match self.axis {
                Axis::X => slot_address.column_index,
                Axis::Y => slot_address.row_index,
            };
            let count = self.value_count(context);
            if count < 2 {
                // The count is 1 if the control unit column/row count fits exactly the size of the matrix.
                // It's 0 if the matrix is even smaller. In both cases, navigation is pointless, so we should return
                // `None`.
                return None;
            }
            Some(AbsoluteValue::Discrete(Fraction::new(
                index as u32,
                count.saturating_sub(1),
            )))
        }
    }

    impl RealearnTarget for PlaytimeControlUnitScrollTarget {
        fn control_type_and_character(
            &self,
            context: ControlContext,
        ) -> (ControlType, TargetCharacter) {
            (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(self.value_count(context)),
                    is_retriggerable: false,
                },
                TargetCharacter::Discrete,
            )
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            let new_index = match value.to_absolute_value()? {
                AbsoluteValue::Continuous(v) => {
                    convert_unit_to_discrete_value(v, self.value_count(context.control_context))
                }
                AbsoluteValue::Discrete(f) => f.actual(),
            };
            let mut unit = context.control_context.unit.borrow_mut();
            let current_value = unit.control_unit_top_left_corner();
            let new_value = match self.axis {
                Axis::X => SlotAddress::new(new_index as usize, current_value.row_index),
                Axis::Y => SlotAddress::new(current_value.column_index, new_index as usize),
            };
            unit.set_control_unit_top_left_corner(new_value);
            Ok(HitResponse::processed_with_effect())
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            context: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            match evt {
                CompoundChangeEvent::ClipMatrix(
                    ClipMatrixEvent::EverythingChanged | ClipMatrixEvent::ControlUnitsChanged,
                ) => (true, None),
                CompoundChangeEvent::Unit(UnitEvent::ControlUnitTopLeftCornerChanged(address)) => {
                    (true, self.calculate_value(context, *address))
                }
                _ => (false, None),
            }
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeControlUnitScroll)
        }

        fn is_available(&self, _: ControlContext) -> bool {
            true
        }
    }

    impl<'a> Target<'a> for PlaytimeControlUnitScrollTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let unit = context.unit.borrow();
            let current_value = unit.control_unit_top_left_corner();
            self.calculate_value(context, current_value)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }
}
