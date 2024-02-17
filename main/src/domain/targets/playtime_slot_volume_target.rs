use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualPlaytimeSlot, DEFAULT_TARGET,
};

use playtime_api::persistence::SlotAddress;

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotVolumeTarget {
    pub slot: VirtualPlaytimeSlot,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotVolumeTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = PlaytimeSlotVolumeTarget {
            slot_coordinates: self.slot.resolve(context, compartment)?,
        };
        Ok(vec![ReaperTarget::PlaytimeSlotVolume(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotVolumeTarget {
    pub slot_coordinates: SlotAddress,
}

pub const PLAYTIME_SLOT_VOLUME_TARGET: TargetTypeDef = TargetTypeDef {
    lua_only: true,
    section: TargetSection::Playtime,
    name: "Slot volume",
    short_name: "Playtime slot volume",
    supports_clip_slot: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeSlotVolumeTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeSlotVolumeTarget {}
    impl<'a> Target<'a> for PlaytimeSlotVolumeTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::playtime_util::interpret_current_clip_slot_value;
    use crate::domain::ui_util::{
        db_unit_value, format_value_as_db, format_value_as_db_without_unit, parse_value_from_db,
        volume_unit_value,
    };
    use crate::domain::{
        Backbone, CompoundChangeEvent, ControlContext, HitResponse, MappingControlContext,
        PlaytimeSlotVolumeTarget, RealearnTarget, ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{
        AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue,
    };
    use playtime_api::persistence::SlotAddress;
    use playtime_clip_engine::{
        base::ClipMatrixEvent,
        rt::{ClipChangeEvent, QualifiedClipChangeEvent},
    };
    use reaper_high::SliderVolume;
    use reaper_medium::Db;
    use std::borrow::Cow;

    impl RealearnTarget for PlaytimeSlotVolumeTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
        }

        fn clip_slot_address(&self) -> Option<SlotAddress> {
            Some(self.slot_coordinates)
        }

        fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
            parse_value_from_db(text)
        }

        fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_db_without_unit(value)
        }

        fn value_unit(&self, _: ControlContext) -> &'static str {
            "dB"
        }

        fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_db(value)
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            let volume =
                SliderVolume::try_from_normalized_slider_value(value.to_unit_value()?.get())
                    .unwrap_or_default();
            let db = volume.db();
            let api_db = playtime_api::persistence::Db::new(db.get())?;
            Backbone::get()
                .with_clip_matrix_mut(
                    context.control_context.instance(),
                    |matrix| -> anyhow::Result<HitResponse> {
                        matrix.set_slot_volume(self.slot_coordinates, api_db)?;
                        Ok(HitResponse::processed_with_effect())
                    },
                )
                .map_err(|_| "couldn't acquire matrix")?
                .map_err(|_| "couldn't carry out volume action")
        }

        fn is_available(&self, _: ControlContext) -> bool {
            // TODO-medium With clip targets we should check the control context (instance state) if
            //  slot filled.
            true
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            _: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                    QualifiedClipChangeEvent {
                        clip_address,
                        event: ClipChangeEvent::Volume(new_value),
                    },
                )) if clip_address.slot_address == self.slot_coordinates => (
                    true,
                    Some(AbsoluteValue::Continuous(db_unit_value(Db::new(
                        new_value.get(),
                    )))),
                ),
                _ => (false, None),
            }
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            Some(self.volume(context)?.to_string().into())
        }

        fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
            Some(NumericValue::Decimal(self.volume(context)?.db().get()))
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeSlotVolume)
        }
    }

    impl PlaytimeSlotVolumeTarget {
        fn volume(&self, context: ControlContext) -> Option<SliderVolume> {
            Backbone::get()
                .with_clip_matrix(context.instance(), |matrix| {
                    let db = matrix.find_slot(self.slot_coordinates)?.volume().ok()?;
                    Some(SliderVolume::from_db(Db::new(db.get())))
                })
                .ok()?
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotVolumeTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let val = self
                .volume(context)
                .map(volume_unit_value)
                .map(AbsoluteValue::Continuous);
            interpret_current_clip_slot_value(val)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }
}
