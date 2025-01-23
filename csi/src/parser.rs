use crate::schema::{Acceleration, Accelerations, Capability, Widget};
use helgoboss_midi::{RawShortMessage, ShortMessageFactory};
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while1, take_while_m_n};
use nom::character::complete::{multispace0, not_line_ending, space0, space1};
use nom::combinator::{all_consuming, map, map_res, opt, verify};
use nom::error::ParseError;
use nom::multi::{separated_list0, separated_list1};
use nom::sequence::{preceded, separated_pair};
use nom::{character::complete::char, sequence::delimited, sequence::tuple, Err, IResult, Parser};
use std::convert::TryInto;

type Res<'a, T> = IResult<&'a str, T>;

pub fn mst_file_content(input: &str) -> Result<Vec<Widget>, String> {
    let non_comment_lines: Vec<_> = input
        .lines()
        .filter(|l| !l.trim_start().starts_with('/'))
        .collect();
    let input_without_comments = non_comment_lines.join("\n");
    let (_, widgets) = all_consuming(widgets)(&input_without_comments).map_err(|e| {
        let short_err = match e {
            Err::Error(e) => Err::Error(nom::error::Error::new(&e.input[0..30], e.code)),
            e => e,
        };
        short_err.to_string()
    })?;
    Ok(widgets)
}

fn widgets(input: &str) -> Res<Vec<Widget>> {
    delimited(
        multispace0,
        separated_list0(space_with_at_least_one_line_ending, widget),
        multispace0,
    )(input)
}

fn widget(input: &str) -> Res<Widget> {
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

fn widget_begin(input: &str) -> Res<&str> {
    preceded(
        tuple((tag("Widget"), space1)),
        take_while1(|ch: char| ch.is_alphanumeric() || matches!(ch, '-' | '_')),
    )(input)
}

fn widget_capabilities(input: &str) -> Res<Vec<Capability>> {
    separated_list0(space_with_at_least_one_line_ending, capability)(input)
}

fn capability(input: &str) -> Res<Capability> {
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

fn capability_press(input: &str) -> Res<Capability> {
    map(util::capability_msg_opt_msg("Press"), |(press, release)| {
        Capability::Press { press, release }
    })(input)
}

fn capability_fb_two_state(input: &str) -> Res<Capability> {
    map(util::capability_msg_msg("FB_TwoState"), |(on, off)| {
        Capability::FbTwoState { on, off }
    })(input)
}

fn capability_fb_encoder(input: &str) -> Res<Capability> {
    map(util::capability_msg("FB_Encoder"), |max| {
        Capability::FbEncoder { max }
    })(input)
}

fn capability_toggle(input: &str) -> Res<Capability> {
    map(util::capability_msg("Toggle"), |on| Capability::Toggle {
        on,
    })(input)
}

fn capability_fader_14_bit(input: &str) -> Res<Capability> {
    map(util::capability_msg("Fader14Bit"), |max| {
        Capability::Fader14Bit { max }
    })(input)
}

fn capability_fb_fader_14_bit(input: &str) -> Res<Capability> {
    map(util::capability_msg("FB_Fader14Bit"), |max| {
        Capability::FbFader14Bit { max }
    })(input)
}

fn capability_touch(input: &str) -> Res<Capability> {
    map(util::capability_msg_msg("Touch"), |(on, off)| {
        Capability::Touch {
            touch: on,
            release: off,
        }
    })(input)
}

fn capability_fb_mcu_display_upper(input: &str) -> Res<Capability> {
    map(util::capability_index("FB_MCUDisplayUpper"), |index| {
        Capability::FbMcuDisplayUpper { index }
    })(input)
}

fn capability_fb_mcu_display_lower(input: &str) -> Res<Capability> {
    map(util::capability_index("FB_MCUDisplayLower"), |index| {
        Capability::FbMcuDisplayLower { index }
    })(input)
}

fn capability_fb_mcu_vu_meter(input: &str) -> Res<Capability> {
    map(util::capability_index("FB_MCUVUMeter"), |index| {
        Capability::FbMcuVuMeter { index }
    })(input)
}

fn capability_fb_mcu_time_display(input: &str) -> Res<Capability> {
    map(util::capability_empty("FB_MCUTimeDisplay"), |_| {
        Capability::FbMcuTimeDisplay
    })(input)
}

fn capability_encoder(input: &str) -> Res<Capability> {
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

fn capability_unknown(input: &str) -> Res<Capability> {
    map(
        verify(not_line_ending, |s: &str| s != "WidgetEnd"),
        |line: &str| Capability::Unknown(line.to_owned()),
    )(input)
}

fn short_midi_msg(input: &str) -> Res<RawShortMessage> {
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

fn accelerations(input: &str) -> Res<Accelerations> {
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

fn acceleration(input: &str) -> Res<Acceleration> {
    alt((acceleration_range, acceleration_sequence))(input)
}

fn acceleration_sequence(input: &str) -> Res<Acceleration> {
    map(separated_list1(space1, hex_byte), |values| {
        Acceleration::Sequence(values)
    })(input)
}

fn acceleration_range(input: &str) -> Res<Acceleration> {
    map(
        separated_pair(hex_byte, char('-'), hex_byte),
        |(min, max)| Acceleration::Range(min..=max),
    )(input)
}

fn hex_byte(input: &str) -> Res<u8> {
    map_res(take_while_m_n(2, 2, util::is_hex_digit), util::from_hex)(input)
}

fn space_with_at_least_one_line_ending(input: &str) -> Res<&str> {
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
        c.is_ascii_hexdigit()
    }

    pub fn from_hex(input: &str) -> Result<u8, std::num::ParseIntError> {
        u8::from_str_radix(input, 16)
    }

    pub fn capability_msg_opt_msg<'a>(
        name: &'static str,
    ) -> impl FnMut(&'a str) -> Res<'a, (RawShortMessage, Option<RawShortMessage>)> {
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
    ) -> impl FnMut(&'a str) -> Res<'a, (RawShortMessage, RawShortMessage)> {
        preceded(
            tag(name),
            tuple((
                preceded(space1, short_midi_msg),
                preceded(space1, short_midi_msg),
            )),
        )
    }

    pub fn capability_index<'a>(name: &'static str) -> impl FnMut(&'a str) -> Res<'a, u8> {
        map_res(preceded(tuple((tag(name), space1)), digit1), |s: &str| {
            s.parse::<u8>()
        })
    }

    pub fn capability_empty<'a>(name: &'static str) -> impl FnMut(&'a str) -> Res<'a, ()> {
        value((), tag(name))
    }

    pub fn capability_msg<'a>(
        name: &'static str,
    ) -> impl FnMut(&'a str) -> Res<'a, RawShortMessage> {
        preceded(tag(name), preceded(space1, short_midi_msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Acceleration, Widget};
    use helgoboss_midi::test_util::u7;
    use helgoboss_midi::ShortMessageFactory;

    #[test]
    fn parse_widgets() {
        let mst_content = include_str!("test_data/test.mst");
        let (_, widgets) = widgets(mst_content).unwrap();
        assert_eq!(widgets.len(), 146);
        for w in widgets {
            for c in w.capabilities {
                assert!(!c.is_unknown());
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
