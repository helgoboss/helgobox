use crate::schema::{Acceleration, Accelerations, Capability, Widget};
use helgoboss_midi::{RawShortMessage, ShortMessageFactory};
use nom::branch::alt;
use nom::bytes::complete::{tag, take_until1, take_while, take_while1, take_while_m_n};
use nom::character::complete::{
    alphanumeric1, hex_digit1, line_ending, multispace0, multispace1, not_line_ending, space0,
    space1,
};
use nom::character::is_alphanumeric;
use nom::combinator::{fail, flat_map, map, map_res, not, opt, peek, recognize, verify};
use nom::error::{context, make_error, ErrorKind, ParseError};
use nom::multi::{many1, many_m_n, separated_list0, separated_list1};
use nom::sequence::{preceded, separated_pair};
use nom::{
    bytes::complete::is_not, character::complete::char, sequence::delimited, sequence::tuple,
    IResult, Parser,
};
use std::convert::TryInto;

pub fn widgets(input: &str) -> IResult<&str, Vec<Widget>> {
    separated_list0(space_with_at_least_one_line_ending, widget)(input)
}

fn widget(input: &str) -> IResult<&str, Widget> {
    map(
        tuple((
            widget_begin,
            space_with_at_least_one_line_ending,
            widget_capabilities,
            space_with_at_least_one_line_ending,
            tag("WidgetEnd"),
        )),
        |(name, _, capabilities, _, _)| Widget {
            name: name.to_owned(),
            capabilities,
        },
    )(input)
}

fn widget_begin(input: &str) -> IResult<&str, &str> {
    preceded(
        tuple((tag("Widget"), space1)),
        take_while1(|ch: char| ch.is_alphanumeric() || matches!(ch, '-' | '_')),
    )(input)
}

fn widget_capabilities(input: &str) -> IResult<&str, Vec<Capability>> {
    separated_list0(space_with_at_least_one_line_ending, capability)(input)
}

fn capability(input: &str) -> IResult<&str, Capability> {
    alt((
        capability_press,
        capability_fb_two_state,
        capability_encoder,
        capability_fb_encoder,
        capability_toggle,
        capability_fader_14_bit,
        capability_fb_fader_14_bit,
        capability_touch,
        capability_fb_mcu_display_upper,
        capability_fb_mcu_display_lower,
        capability_fb_mcu_vu_meter,
        capability_fb_mcu_time_display,
        capability_unknown,
    ))(input)
}

fn capability_press(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg_opt_msg("Press"), |(press, release)| {
        Capability::Press { press, release }
    })(input)
}

fn capability_fb_two_state(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg_msg("FB_TwoState"), |(on, off)| {
        Capability::FbTwoState { on, off }
    })(input)
}

fn capability_fb_encoder(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg("FB_Encoder"), |max| {
        Capability::FbEncoder { max }
    })(input)
}

fn capability_toggle(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg("Toggle"), |on| Capability::Toggle {
        on,
    })(input)
}

fn capability_fader_14_bit(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg("Fader14Bit"), |max| {
        Capability::Fader14Bit { max }
    })(input)
}

fn capability_fb_fader_14_bit(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg("FB_Fader14Bit"), |max| {
        Capability::FbFader14Bit { max }
    })(input)
}

fn capability_touch(input: &str) -> IResult<&str, Capability> {
    map(util::capability_msg_msg("Touch"), |(on, off)| {
        Capability::Touch { on, off }
    })(input)
}

fn capability_fb_mcu_display_upper(input: &str) -> IResult<&str, Capability> {
    map(util::capability_index("FB_MCUDisplayUpper"), |index| {
        Capability::FbMcuDisplayUpper { index }
    })(input)
}

fn capability_fb_mcu_display_lower(input: &str) -> IResult<&str, Capability> {
    map(util::capability_index("FB_MCUDisplayLower"), |index| {
        Capability::FbMcuDisplayLower { index }
    })(input)
}

fn capability_fb_mcu_vu_meter(input: &str) -> IResult<&str, Capability> {
    map(util::capability_index("FB_MCUVUMeter"), |index| {
        Capability::FbMcuVuMeter { index }
    })(input)
}

fn capability_fb_mcu_time_display(input: &str) -> IResult<&str, Capability> {
    map(util::capability_empty("FB_MCUTimeDisplay"), |_| {
        Capability::FbMcuTimeDisplay
    })(input)
}

fn capability_encoder(input: &str) -> IResult<&str, Capability> {
    map(
        tuple((
            preceded(tuple((tag("Encoder"), space1)), short_midi_msg),
            opt(preceded(space1, accelerations)),
        )),
        |(main, accelerations)| Capability::Encoder {
            main,
            accelerations,
        },
    )(input)
}

/// TODO-high Factor out
pub fn dbg_dmp<'a, F, O, E: std::fmt::Debug>(
    f: F,
    context: &'static str,
) -> impl Fn(&'a str) -> IResult<&'a str, O, E>
where
    F: Fn(&'a str) -> IResult<&'a str, O, E>,
{
    move |i: &'a str| match f(i) {
        Err(e) => {
            println!("{}: Error({:?}) at:\n{}", context, e, i);
            Err(e)
        }
        a => a,
    }
}

fn capability_unknown(input: &str) -> IResult<&str, Capability> {
    map(
        verify(not_line_ending, |s: &str| s != "WidgetEnd"),
        |line: &str| Capability::Unknown(line.to_owned()),
    )(input)
}

fn short_midi_msg(input: &str) -> IResult<&str, RawShortMessage> {
    map_res(
        tuple((hex_byte, space1, hex_byte, space1, hex_byte)),
        |(b1, _, b2, _, b3)| {
            RawShortMessage::from_bytes((
                b1,
                b2.try_into().map_err(|_| "data byte 1 too high")?,
                b3.try_into().map_err(|_| "data byte 2 too high")?,
            ))
            .map_err(|_| "invalid short message")
        },
    )(input)
}

fn accelerations(input: &str) -> IResult<&str, Accelerations> {
    map(
        delimited(
            ws(char('[')),
            tuple((
                parameterized_acceleration('<'),
                parameterized_acceleration('>'),
            )),
            ws(char(']')),
        ),
        |(decrements, increments)| Accelerations {
            increments,
            decrements,
        },
    )(input)
}

fn parameterized_acceleration<'a>(
    letter: char,
) -> impl FnMut(&'a str) -> IResult<&'a str, Acceleration> {
    preceded(ws(char(letter)), acceleration)
}

fn acceleration(input: &str) -> IResult<&str, Acceleration> {
    alt((acceleration_range, acceleration_sequence))(input)
}

fn acceleration_sequence(input: &str) -> IResult<&str, Acceleration> {
    map(separated_list1(space1, hex_byte), |values| {
        Acceleration::Sequence(values)
    })(input)
}

fn acceleration_range(input: &str) -> IResult<&str, Acceleration> {
    map(
        separated_pair(hex_byte, char('-'), hex_byte),
        |(min, max)| Acceleration::Range(min..=max),
    )(input)
}

fn hex_byte(input: &str) -> IResult<&str, u8> {
    map_res(take_while_m_n(2, 2, util::is_hex_digit), util::from_hex)(input)
}

fn space_with_at_least_one_line_ending(input: &str) -> IResult<&str, &str> {
    verify(multispace0, |s: &str| s.contains(&['\r', '\n'][..]))(input)
}

/// Surrounded by optional whitespace (no line endings).
fn ws<'a, O, E>(p: impl Parser<&'a str, O, E>) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
where
    E: ParseError<&'a str>,
{
    delimited(space0, p, space0)
}

mod util {
    use super::*;
    use nom::character::complete::digit1;
    use nom::combinator::value;

    pub fn is_hex_digit(c: char) -> bool {
        c.is_digit(16)
    }

    pub fn from_hex(input: &str) -> Result<u8, std::num::ParseIntError> {
        u8::from_str_radix(input, 16)
    }

    pub fn capability_msg_opt_msg<'a>(
        name: &'static str,
    ) -> impl FnMut(&'a str) -> IResult<&str, (RawShortMessage, Option<RawShortMessage>)> {
        preceded(
            tag(name),
            tuple((
                preceded(space1, short_midi_msg),
                opt(preceded(space1, short_midi_msg)),
            )),
        )
    }

    pub fn capability_msg_msg<'a>(
        name: &'static str,
    ) -> impl FnMut(&'a str) -> IResult<&str, (RawShortMessage, RawShortMessage)> {
        preceded(
            tag(name),
            tuple((
                preceded(space1, short_midi_msg),
                preceded(space1, short_midi_msg),
            )),
        )
    }

    pub fn capability_index<'a>(name: &'static str) -> impl FnMut(&'a str) -> IResult<&str, u32> {
        map_res(preceded(tuple((tag(name), space1)), digit1), |s: &str| {
            u32::from_str_radix(s, 10)
        })
    }

    pub fn capability_empty<'a>(name: &'static str) -> impl FnMut(&'a str) -> IResult<&str, ()> {
        value((), tag(name))
    }

    pub fn capability_msg<'a>(
        name: &'static str,
    ) -> impl FnMut(&'a str) -> IResult<&str, RawShortMessage> {
        preceded(tag(name), preceded(space1, short_midi_msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Acceleration, Widget};
    use helgoboss_midi::test_util::u7;
    use helgoboss_midi::ShortMessageFactory;
    use nom::character::complete::{digit1, multispace0};
    use nom::error::ParseError;
    use nom::sequence::pair;
    use nom::InputLength;
    use std::any::Any;
    use std::fmt::Debug;

    #[test]
    fn parse_widgets() {
        let mst_content = include_str!("test_data/test.mst");
        let (_, widgets) = widgets(mst_content).unwrap();
        assert_eq!(widgets.len(), 146);
        for w in widgets {
            for c in w.capabilities {
                assert!(!matches!(c, Capability::Unknown(_)))
            }
        }
    }

    #[test]
    fn parse_widget() {
        assert_eq!(
            widget(
                "\
Widget RecordArm1
    Press 90 00 7f 90 00 00
    FB_TwoState 90 00 7f 90 00 00
    Weird eu 898 dqwun wd08 . ---
WidgetEnd"
            ),
            Ok((
                "",
                Widget {
                    name: "RecordArm1".to_owned(),
                    capabilities: vec![
                        Capability::Press {
                            press: short(0x90, 0x00, 0x7f),
                            release: Some(short(0x90, 0x00, 0x00)),
                        },
                        Capability::FbTwoState {
                            on: short(0x90, 0x00, 0x7f),
                            off: short(0x90, 0x00, 0x00),
                        },
                        Capability::Unknown("Weird eu 898 dqwun wd08 . ---".to_owned())
                    ]
                }
            ))
        );
    }

    #[test]
    fn parse_widget_ugly_formatting() {
        assert_eq!(
            widget(
                "\
Widget     RecordArm1

         Press 90 00 7f 90 00 00

    FB_TwoState 90 00 7f 90 00 00
Weird eu 898 dqwun wd08 . ---


WidgetEnd"
            ),
            Ok((
                "",
                Widget {
                    name: "RecordArm1".to_owned(),
                    capabilities: vec![
                        Capability::Press {
                            press: short(0x90, 0x00, 0x7f),
                            release: Some(short(0x90, 0x00, 0x00)),
                        },
                        Capability::FbTwoState {
                            on: short(0x90, 0x00, 0x7f),
                            off: short(0x90, 0x00, 0x00),
                        },
                        Capability::Unknown("Weird eu 898 dqwun wd08 . ---".to_owned())
                    ]
                }
            ))
        );
    }

    #[test]
    fn parse_press_capability_without_release() {
        assert_eq!(
            capability("Press 90 28 7f"),
            Ok((
                "",
                Capability::Press {
                    press: short(0x90, 0x28, 0x7f),
                    release: None,
                }
            ))
        );
    }

    #[test]
    fn parse_press_capability_with_release() {
        assert_eq!(
            capability("Press 90 28 7f 90 28 00"),
            Ok((
                "",
                Capability::Press {
                    press: short(0x90, 0x28, 0x7f),
                    release: Some(short(0x90, 0x28, 0x00)),
                }
            ))
        );
    }

    #[test]
    fn parse_fb_two_state_capability() {
        assert_eq!(
            capability("FB_TwoState 90 00 7f 90 00 00"),
            Ok((
                "",
                Capability::FbTwoState {
                    on: short(0x90, 0x00, 0x7f),
                    off: short(0x90, 0x00, 0x00),
                }
            ))
        );
    }

    #[test]
    fn parse_encoder_capability_with_range() {
        assert_eq!(
            capability("Encoder b0 10 7f [ < 41-48 > 01-08 ]"),
            Ok((
                "",
                Capability::Encoder {
                    main: short(0xb0, 0x10, 0x7f),
                    accelerations: Some(Accelerations {
                        decrements: Acceleration::Range(0x41..=0x48),
                        increments: Acceleration::Range(0x01..=0x08)
                    })
                }
            ))
        );
    }

    #[test]
    fn parse_short_midi_msg() {
        assert_eq!(
            short_midi_msg("90 28 7f"),
            Ok(("", short(0x90, 0x28, 0x7f)))
        );
    }

    #[test]
    fn single_hex_byte() {
        assert_eq!(hex_byte("90"), Ok(("", 0x90)));
    }

    fn short(status_byte: u8, data_byte_1: u8, data_byte_2: u8) -> RawShortMessage {
        RawShortMessage::from_bytes((status_byte, u7(data_byte_1), u7(data_byte_2))).unwrap()
    }
}
