use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};

use realearn_api::persistence::Axis;

#[derive(Debug)]
pub struct UnresolvedPlaytimeBrowseCellsTarget {
    pub axis: Axis,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeBrowseCellsTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = PlaytimeBrowseCellsTarget { axis: self.axis };
        Ok(vec![ReaperTarget::PlaytimeBrowseCells(target)])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeBrowseCellsTarget {
    axis: Axis,
}

pub const PLAYTIME_BROWSE_CELLS_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Browse cells",
    short_name: "Playtime browse cells",
    supports_axis: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeBrowseCellsTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeBrowseCellsTarget {}
    impl<'a> Target<'a> for PlaytimeBrowseCellsTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        convert_count_to_step_size, convert_unit_to_discrete_value_with_none, CompoundChangeEvent,
        ControlContext, HitResponse, MappingControlContext, PlaytimeBrowseCellsTarget,
        RealearnTarget, ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};
    use playtime_api::runtime::CellAddress;
    #[cfg(feature = "playtime")]
    use playtime_clip_engine::base::ClipMatrixEvent;
    use realearn_api::persistence::Axis;

    impl PlaytimeBrowseCellsTarget {
        fn column_or_row_count(&self, context: ControlContext) -> u32 {
            context
                .instance()
                .borrow()
                .clip_matrix()
                .map(|m| match self.axis {
                    Axis::X => m.column_count(),
                    Axis::Y => m.row_count(),
                } as u32)
                .unwrap_or(0)
        }

        fn calculate_value(
            &self,
            context: ControlContext,
            cell_address: CellAddress,
        ) -> Option<AbsoluteValue> {
            let index = match self.axis {
                Axis::X => cell_address.column_index,
                Axis::Y => cell_address.row_index,
            };
            let v = if let Some(i) = index { i + 1 } else { 0 };
            let fraction = Fraction::new(v as u32, self.column_or_row_count(context));
            Some(AbsoluteValue::Discrete(fraction))
        }
    }

    impl RealearnTarget for PlaytimeBrowseCellsTarget {
        fn control_type_and_character(
            &self,
            context: ControlContext,
        ) -> (ControlType, TargetCharacter) {
            (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(
                        self.column_or_row_count(context) + 1,
                    ),
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
            let new_value = match value.to_absolute_value()? {
                AbsoluteValue::Continuous(v) => convert_unit_value_to_target_value(
                    v,
                    self.column_or_row_count(context.control_context),
                ),
                AbsoluteValue::Discrete(f) => f.actual(),
            };
            let new_index = if new_value == 0 {
                None
            } else {
                Some(new_value as usize - 1)
            };
            let mut instance = context.control_context.instance.borrow_mut();
            let matrix = instance.clip_matrix_mut().ok_or("no matrix")?;
            let current_cell = matrix.active_cell();
            let new_cell = match self.axis {
                Axis::X => CellAddress::new(new_index, current_cell.row_index),
                Axis::Y => CellAddress::new(current_cell.column_index, new_index),
            };
            matrix
                .activate_cell(new_cell)
                .map_err(|_| "cell doesn't exist")?;
            Ok(HitResponse::processed_with_effect())
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            _context: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            match evt {
                CompoundChangeEvent::ClipMatrix(
                    ClipMatrixEvent::EverythingChanged | ClipMatrixEvent::ActiveCellChanged,
                ) => (true, None),
                _ => (false, None),
            }
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeBrowseCells)
        }

        fn is_available(&self, _: ControlContext) -> bool {
            true
        }

        fn parse_as_value(
            &self,
            text: &str,
            context: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            self.parse_value_from_discrete_value(text, context)
        }

        fn parse_as_step_size(
            &self,
            text: &str,
            context: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            self.parse_value_from_discrete_value(text, context)
        }

        fn convert_unit_value_to_discrete_value(
            &self,
            input: UnitValue,
            context: ControlContext,
        ) -> Result<u32, &'static str> {
            let value =
                convert_unit_value_to_target_value(input, self.column_or_row_count(context));
            Ok(value)
        }
    }

    impl<'a> Target<'a> for PlaytimeBrowseCellsTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let instance = context.instance.borrow();
            let matrix = instance.clip_matrix()?;
            let active_cell = matrix.active_cell();
            self.calculate_value(context, active_cell)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }

    fn convert_unit_value_to_target_value(input: UnitValue, column_or_row_count: u32) -> u32 {
        convert_unit_to_discrete_value_with_none(input, column_or_row_count)
            .map(|i| i + 1)
            .unwrap_or(0)
    }
}
