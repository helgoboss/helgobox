use helgoboss_learn::{format_percentage_without_unit, parse_percentage_without_unit, UnitValue};
use reaper_high::{FxParameter, Reaper, Volume};
use reaper_medium::{Db, ReaperNormalizedFxParamValue, ReaperVolumeValue};
use slog::warn;
use std::convert::TryInto;

pub fn format_as_percentage_without_unit(value: UnitValue) -> String {
    format_percentage_without_unit(value.get())
}

pub fn format_as_symmetric_percentage_without_unit(value: UnitValue) -> String {
    let symmetric_unit_value = value.get() * 2.0 - 1.0;
    format_percentage_without_unit(symmetric_unit_value)
}

pub fn format_as_double_percentage_without_unit(value: UnitValue) -> String {
    let double_unit_value = value.get() * 2.0;
    format_percentage_without_unit(double_unit_value)
}

pub fn parse_unit_value_from_percentage(text: &str) -> Result<UnitValue, &'static str> {
    parse_percentage_without_unit(text)?.try_into()
}

pub fn parse_from_symmetric_percentage(text: &str) -> Result<UnitValue, &'static str> {
    let percentage: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
    let symmetric_unit_value = percentage / 100.0;
    ((symmetric_unit_value + 1.0) / 2.0).try_into()
}

pub fn parse_from_double_percentage(text: &str) -> Result<UnitValue, &'static str> {
    let percentage: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
    let doble_unit_value = percentage / 100.0;
    (doble_unit_value / 2.0).try_into()
}

pub fn parse_value_from_db(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let db: Db = decimal.try_into().map_err(|_| "not in dB range")?;
    Volume::from_db(db).soft_normalized_value().try_into()
}

pub fn format_value_as_db_without_unit(value: UnitValue) -> String {
    let db = Volume::try_from_soft_normalized_value(value.get())
        .unwrap_or(Volume::MIN)
        .db();
    if db == Db::MINUS_INF {
        "-inf".to_string()
    } else {
        format!("{:.2}", db.get())
    }
}

pub fn reaper_volume_unit_value(volume: ReaperVolumeValue) -> UnitValue {
    volume_unit_value(Volume::from_reaper_value(volume))
}

pub fn volume_unit_value(volume: Volume) -> UnitValue {
    // The soft-normalized value can be > 1.0, e.g. when we have a volume of 12 dB and then
    // lower the volume fader limit to a lower value. In that case we just report the
    // highest possible value ... not much else we can do.
    UnitValue::new_clamped(volume.soft_normalized_value())
}

pub fn convert_bool_to_unit_value(on: bool) -> UnitValue {
    if on {
        UnitValue::MAX
    } else {
        UnitValue::MIN
    }
}

pub fn fx_parameter_unit_value(
    param: &FxParameter,
    value: ReaperNormalizedFxParamValue,
) -> UnitValue {
    let v = value.get();
    if !UnitValue::is_valid(v) {
        // Either the FX reports a wrong value range (e.g. TAL Flanger Sync Speed)
        // or the value range exceeded a "normal" range (e.g. ReaPitch Wet). We can't
        // know. In future, we might offer further customization possibilities here.
        // For now, we just report it as 0.0 or 1.0 and log a warning.
        warn!(
            Reaper::get().logger(),
            "FX parameter reported normalized value {:?} which is not in unit interval: {:?}",
            v,
            param
        );
        return UnitValue::new_clamped(v);
    }
    UnitValue::new(v)
}

pub fn format_value_as_db(value: UnitValue) -> String {
    Volume::try_from_soft_normalized_value(value.get())
        .unwrap_or(Volume::MIN)
        .to_string()
}
