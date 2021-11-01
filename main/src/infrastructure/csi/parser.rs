use crate::infrastructure::csi::schema::{Capability, Widget};
use helgoboss_midi::{RawShortMessage, ShortMessageFactory};
use nom::branch::alt;
use nom::bytes::complete::{tag, take_until1, take_while, take_while1, take_while_m_n};
use nom::character::complete::{
    alphanumeric1, hex_digit1, line_ending, multispace1, not_line_ending, space0, space1,
};
use nom::character::is_alphanumeric;
use nom::combinator::{fail, flat_map, map, map_res, not, opt, peek, recognize};
use nom::error::{make_error, ErrorKind};
use nom::multi::{many1, many_m_n, separated_list0};
use nom::sequence::{preceded, separated_pair};
use nom::{
    bytes::complete::is_not, character::complete::char, sequence::delimited, sequence::tuple,
    IResult, Parser,
};
use std::convert::TryInto;

fn widgets(input: &str) -> IResult<&str, Vec<Widget>> {
    separated_list0(line_endings_with_space, widget)(input)
}

fn line_endings_with_space(input: &str) -> IResult<&str, &str> {
    recognize(tuple((space0, many1(line_ending), space0)))(input)
}

fn widget(input: &str) -> IResult<&str, Widget> {
    map(
        tuple((
            widget_begin,
            line_endings_with_space,
            widget_capabilities,
            line_endings_with_space,
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
    separated_list0(line_endings_with_space, capability)(input)
}

fn capability(input: &str) -> IResult<&str, Capability> {
    alt((
        capability_press,
        capability_fb_two_state,
        capability_fb_encoder,
        capability_unknown,
    ))(input)
}

fn capability_press(input: &str) -> IResult<&str, Capability> {
    map(util::capability_req_opt("Press"), |(press, release)| {
        Capability::Press { press, release }
    })(input)
}

fn capability_fb_two_state(input: &str) -> IResult<&str, Capability> {
    map(util::capability_req_req("FB_TwoState"), |(on, off)| {
        Capability::FbTwoState { on, off }
    })(input)
}

fn capability_fb_encoder(input: &str) -> IResult<&str, Capability> {
    map(util::capability_req("FB_Encoder"), |max| {
        Capability::FbEncoder { max }
    })(input)
}

fn capability_unknown(input: &str) -> IResult<&str, Capability> {
    let (input, peeked) = peek(alphanumeric1)(input)?;
    if peeked == "WidgetEnd" {
        fail(input)
    } else {
        map(not_line_ending, |line: &str| {
            Capability::Unknown(line.to_owned())
        })(input)
    }
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

fn hex_byte(input: &str) -> IResult<&str, u8> {
    map_res(take_while_m_n(2, 2, util::is_hex_digit), util::from_hex)(input)
}

mod util {
    use super::*;

    pub fn is_hex_digit(c: char) -> bool {
        c.is_digit(16)
    }

    pub fn from_hex(input: &str) -> Result<u8, std::num::ParseIntError> {
        u8::from_str_radix(input, 16)
    }

    pub fn capability_req_opt<'a>(
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

    pub fn capability_req_req<'a>(
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

    pub fn capability_req<'a>(
        name: &'static str,
    ) -> impl FnMut(&'a str) -> IResult<&str, RawShortMessage> {
        preceded(tag(name), preceded(space1, short_midi_msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::csi::schema::Widget;
    use helgoboss_midi::test_util::u7;
    use helgoboss_midi::ShortMessageFactory;

    #[test]
    fn parse_widgets() {
        let mst_content = include_str!("tests/X-Touch_One.mst");
        println!("{:#?}", widgets(mst_content).unwrap());
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
