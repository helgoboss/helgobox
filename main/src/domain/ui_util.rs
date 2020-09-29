use helgoboss_learn::UnitValue;
use std::convert::TryInto;

pub fn format_as_percentage_without_unit(value: UnitValue) -> String {
    let percent = value.get() * 100.0;
    if (percent - percent.round()).abs() < 0.001 {
        // No fraction. Omit zeros after dot.
        format!("{:.0}", percent)
    } else {
        // Has fraction. We want to display these.
        format!("{:.2}", percent)
    }
}

pub fn parse_from_percentage(text: &str) -> Result<UnitValue, &'static str> {
    let percentage: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
    (percentage / 100.0).try_into()
}
