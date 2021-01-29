use helgoboss_learn::{MidiSource, MidiSourceValue, SourceCharacter};
use helgoboss_midi::{
    Channel, ControlChange14BitMessageScanner, ControllerNumber,
    PollingParameterNumberMessageScanner, RawShortMessage, ShortMessage, StructuredShortMessage,
    U7,
};
use reaper_medium::MidiInputDeviceId;
use std::cmp::Ordering;
use std::time::{Duration, Instant};

const MAX_CC_MSG_COUNT: usize = 10;
const MAX_CC_WAITING_TIME: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub struct MidiSourceScanner {
    // Scanners for more complex MIDI message types
    nrpn_scanner: PollingParameterNumberMessageScanner,
    cc_14_bit_scanner: ControlChange14BitMessageScanner,
    state: State,
    dev_id: Option<MidiInputDeviceId>,
}

impl Default for MidiSourceScanner {
    fn default() -> Self {
        Self {
            nrpn_scanner: PollingParameterNumberMessageScanner::new(Duration::from_millis(1)),
            cc_14_bit_scanner: Default::default(),
            state: State::Initial,
            dev_id: None,
        }
    }
}

#[derive(Debug)]
enum State {
    Initial,
    WaitingForMoreCcMsgs(ControlChangeState),
}

#[derive(Debug)]
struct ControlChangeState {
    start_time: Instant,
    channel: Channel,
    controller_number: ControllerNumber,
    msg_count: usize,
    values: [U7; MAX_CC_MSG_COUNT],
}

impl ControlChangeState {
    fn new(channel: Channel, controller_number: ControllerNumber) -> ControlChangeState {
        ControlChangeState {
            start_time: Instant::now(),
            channel,
            controller_number,
            msg_count: 0,
            values: [U7::MIN; MAX_CC_MSG_COUNT],
        }
    }

    fn add_value(&mut self, value: U7) {
        assert!(self.msg_count < MAX_CC_MSG_COUNT);
        self.values[self.msg_count] = value;
        self.msg_count += 1;
    }

    fn time_to_guess(&self) -> bool {
        self.msg_count >= MAX_CC_MSG_COUNT || Instant::now() - self.start_time > MAX_CC_WAITING_TIME
    }

    fn matches(&self, channel: Channel, controller_number: ControllerNumber) -> bool {
        channel == self.channel && controller_number == self.controller_number
    }
}

impl MidiSourceScanner {
    pub fn feed_short(
        &mut self,
        msg: RawShortMessage,
        dev_id: Option<MidiInputDeviceId>,
    ) -> Option<MidiSource> {
        if let Some(nrpn_msg) = self.nrpn_scanner.feed(&msg) {
            let res = self.feed(
                MidiSourceValue::<RawShortMessage>::ParameterNumber(nrpn_msg),
                dev_id,
            );
            if res.is_some() {
                return res;
            }
        }
        if let Some(cc14_msg) = self.cc_14_bit_scanner.feed(&msg) {
            let res = self.feed(
                MidiSourceValue::<RawShortMessage>::ControlChange14Bit(cc14_msg),
                dev_id,
            );
            if res.is_some() {
                return res;
            }
        }
        self.feed(MidiSourceValue::Plain(msg), dev_id)
    }

    pub(crate) fn feed(
        &mut self,
        source_value: MidiSourceValue<RawShortMessage>,
        dev_id: Option<MidiInputDeviceId>,
    ) -> Option<MidiSource> {
        // First encountered device ID rules.
        if self.dev_id.is_none() {
            self.dev_id = dev_id;
        }
        match &mut self.state {
            State::Initial => {
                if let MidiSourceValue::Plain(msg) = source_value {
                    if let StructuredShortMessage::ControlChange {
                        channel,
                        controller_number,
                        control_value,
                    } = msg.to_structured()
                    {
                        let mut cc_state = ControlChangeState::new(channel, controller_number);
                        cc_state.add_value(control_value);
                        self.state = State::WaitingForMoreCcMsgs(cc_state);
                        None
                    } else {
                        MidiSource::from_source_value(source_value)
                    }
                } else {
                    MidiSource::from_source_value(source_value)
                }
            }
            State::WaitingForMoreCcMsgs(cc_state) => {
                if let MidiSourceValue::Plain(msg) = source_value {
                    if let StructuredShortMessage::ControlChange {
                        channel,
                        controller_number,
                        control_value,
                    } = msg.to_structured()
                    {
                        if cc_state.matches(channel, controller_number) {
                            cc_state.add_value(control_value);
                        }
                    }
                    Some(self.guess_or_not()?.0)
                } else {
                    // Looks like in the meantime, the composite scanners ((N)RPN or
                    // 14-bit CC) have figured out that the combination is a composite
                    // message. This fixes https://github.com/helgoboss/realearn/issues/95.
                    let source = MidiSource::from_source_value(source_value);
                    if source.is_some() {
                        self.reset();
                    }
                    source
                }
            }
        }
    }

    pub fn poll(&mut self) -> Option<(MidiSource, Option<MidiInputDeviceId>)> {
        for ch in 0..16 {
            if let Some(nrpn_msg) = self.nrpn_scanner.poll(Channel::new(ch)) {
                let source_value = MidiSourceValue::<RawShortMessage>::ParameterNumber(nrpn_msg);
                if let Some(source) = self.feed(source_value, None) {
                    return Some((source, self.dev_id));
                }
            }
        }
        self.guess_or_not()
    }

    pub fn reset(&mut self) {
        self.nrpn_scanner.reset();
        self.cc_14_bit_scanner.reset();
        self.state = State::Initial;
    }

    fn guess_or_not(&mut self) -> Option<(MidiSource, Option<MidiInputDeviceId>)> {
        if let State::WaitingForMoreCcMsgs(cc_state) = &self.state {
            if cc_state.time_to_guess() {
                let guessed_source = guess_source(cc_state);
                self.reset();
                Some((guessed_source, self.dev_id))
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn guess_source(cc_state: &ControlChangeState) -> MidiSource {
    MidiSource::ControlChangeValue {
        channel: Some(cc_state.channel),
        controller_number: Some(cc_state.controller_number),
        custom_character: guess_custom_character(&cc_state.values[0..cc_state.msg_count - 1]),
    }
}

fn contains_direction_change(values: &[U7]) -> bool {
    #[derive(Copy, Clone, PartialEq)]
    enum Direction {
        Clockwise,
        CounterClockwise,
    }
    fn determine_direction(a: U7, b: U7) -> Option<Direction> {
        use Direction::*;
        use Ordering::*;
        match b.cmp(&a) {
            Greater => Some(Clockwise),
            Less => Some(CounterClockwise),
            Equal => None,
        }
    }
    let mut direction_so_far: Option<Direction> = None;
    for i in 1..values.len() {
        let new_direction = determine_direction(values[i - 1], values[i]);
        if new_direction.is_none() {
            continue;
        }
        if direction_so_far.is_none() {
            direction_so_far = new_direction;
            continue;
        }
        if new_direction != direction_so_far {
            return true;
        }
    }
    false
}

fn contains_consecutive_duplicates(values: &[U7]) -> bool {
    for i in 1..values.len() {
        if values[i] == values[i - 1] {
            return true;
        }
    }
    false
}

fn guess_custom_character(values: &[U7]) -> SourceCharacter {
    use SourceCharacter::*;
    // We don't just interpret 127 or 100 as button because we consider typical keyboard keys also
    // as buttons. They can be velocity-sensitive and therefore transmit any value.
    #[allow(clippy::if_same_then_else)]
    if values.len() == 1 {
        // Only one message received. Looks like a button has been pressed and not released.
        Button
    } else if values.len() == 2 && values[1] == U7::MIN {
        // Two messages received and second message has value 0. Looks like a button has been
        // pressed and released.
        Button
    } else {
        // Multiple messages received. Button character is ruled out already. Check continuity.
        if contains_direction_change(values) {
            // A direction change means it's very likely a (relative) encoder.
            guess_encoder_type(values)
        } else if contains_consecutive_duplicates(values) {
            if values.contains(&U7::MIN) {
                // For relative, zero means "don't do anything" - which is a bit pointless
                // to send. So it's probably an encoder which is
                // configured to transmit absolute values hitting
                // the lower boundary.
                Range
            } else if values.contains(&U7::MAX) {
                // Here we rely on the fact that the user should turn clock-wise. So it
                // can't be relative type 1 because 127 means
                // decrement. It's also unlikely to be the
                // other relative types because this would happen with extreme acceleration
                // only. So it's probably an encoder which is configured to transmit
                // absolute values hitting the upper boundary.
                Range
            } else {
                guess_encoder_type(values)
            }
        } else {
            // Was continuous without duplicates until now so it's probably a knob/fader.
            SourceCharacter::Range
        }
    }
}

/// Unfortunately, encoder type 3 clockwise movement is not really distinguishable from 1 or 2.
/// So we won't support its detection.
fn guess_encoder_type(values: &[U7]) -> SourceCharacter {
    use SourceCharacter::*;
    match values[0].get() {
        1..=7 | 121..=127 => Encoder1,
        57..=71 => Encoder2,
        // The remaining values are supported but not so typical for encoders because they only
        // happen at high accelerations.
        _ => Range,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod scanning {
        use super::*;
        use helgoboss_midi::test_util::{channel, control_change, u14};
        use helgoboss_midi::{ParameterNumberMessage, ParameterNumberMessageScanner};

        #[test]
        fn scan_nrpn() {
            // Given
            let mut source_scanner = MidiSourceScanner::default();
            let mut nrpn_scanner = ParameterNumberMessageScanner::new();
            // When
            use MidiSourceValue::{ParameterNumber, Plain};
            // Message 1
            let msg_1 = control_change(1, 99, 0);
            let nrpn_1 = nrpn_scanner.feed(&msg_1);
            assert_eq!(nrpn_1, None);
            let source_1 = source_scanner.feed(Plain(msg_1), None);
            // Message 2
            let msg_2 = control_change(1, 98, 99);
            let nrpn_2 = nrpn_scanner.feed(&msg_2);
            let source_2 = source_scanner.feed(Plain(msg_1), None);
            assert_eq!(nrpn_2, None);
            // Message 3
            let msg_3 = control_change(1, 38, 3);
            let nrpn_3 = nrpn_scanner.feed(&msg_3);
            assert_eq!(nrpn_3, None);
            let source_3 = source_scanner.feed(Plain(msg_3), None);
            // Message 4
            let msg_4 = control_change(1, 6, 2);
            let nrpn_4 = nrpn_scanner.feed(&msg_4).unwrap();
            assert_eq!(
                nrpn_4,
                ParameterNumberMessage::non_registered_14_bit(channel(1), u14(99), u14(259))
            );
            let source_4_nrpn = source_scanner.feed(ParameterNumber(nrpn_4), None);
            let source_4_short = source_scanner.feed(Plain(msg_4), None);
            // Then
            // Even our source scanner is already waiting for more CC messages with the same number,
            // a suddenly arriving (N)RPN message should take precedence! Because our real-time
            // processor constantly scans for (N)RPN, it would detect at some point that this looks
            // like a valid (N)RPN message. This needs to happen *before* the 250 millis
            // MAX_CC_WAITING_TIME have expired. In practice this is always the case because there
            // should never be much delay between the single messages making up one (N)RPN message.
            assert_eq!(source_1, None);
            assert_eq!(source_2, None);
            assert_eq!(source_3, None);
            assert_eq!(
                source_4_nrpn.unwrap(),
                MidiSource::ParameterNumberValue {
                    channel: Some(channel(1)),
                    number: Some(u14(99)),
                    is_14_bit: Some(true),
                    is_registered: Some(false),
                }
            );
            assert_eq!(source_4_short, None);
        }
    }

    mod source_character_guessing {
        use super::*;
        use helgoboss_midi::test_util::u7;
        use SourceCharacter::*;

        #[test]
        fn typical_range() {
            assert_eq!(guess(&[40, 41, 42, 43, 44]), Range);
        }

        #[test]
        fn typical_range_counter_clockwise() {
            assert_eq!(guess(&[44, 43, 42, 41, 40]), Range);
        }

        #[test]
        fn typical_trigger_button() {
            assert_eq!(guess(&[100]), Button);
            assert_eq!(guess(&[127]), Button);
        }

        #[test]
        fn typical_switch_button() {
            assert_eq!(guess(&[100, 0]), Button);
            assert_eq!(guess(&[127, 0]), Button);
        }

        #[test]
        fn typical_encoder_1() {
            assert_eq!(guess(&[1, 1, 1, 1, 1]), Encoder1);
        }

        #[test]
        fn typical_encoder_2() {
            assert_eq!(guess(&[65, 65, 65, 65, 65]), Encoder2);
        }

        #[test]
        fn typical_encoder_2_counter_clockwise() {
            assert_eq!(guess(&[63, 63, 63, 63, 63]), Encoder2);
        }

        #[test]
        fn velocity_sensitive_trigger_button() {
            assert_eq!(guess(&[79]), Button);
            assert_eq!(guess(&[10]), Button);
        }

        #[test]
        fn velocity_sensitive_switch_button() {
            assert_eq!(guess(&[79, 0]), Button);
            assert_eq!(guess(&[10, 0]), Button);
        }

        #[test]
        fn range_with_gaps() {
            assert_eq!(guess(&[40, 42, 43, 46]), Range);
        }

        #[test]
        fn range_with_gaps_counter_clockwise() {
            assert_eq!(guess(&[44, 41, 40, 37, 35]), Range);
        }

        #[test]
        fn very_lower_range() {
            assert_eq!(guess(&[0, 1, 2, 3]), Range);
        }

        #[test]
        fn lower_range() {
            assert_eq!(guess(&[1, 2, 3, 4]), Range);
        }

        #[test]
        fn very_upper_range_counter_clockwise() {
            assert_eq!(guess(&[127, 126, 125, 124]), Range);
        }

        #[test]
        fn upper_range_counter_clockwise() {
            assert_eq!(guess(&[126, 125, 124, 123]), Range);
        }

        #[test]
        fn encoder_1_with_acceleration() {
            assert_eq!(guess(&[1, 2, 2, 1, 1]), Encoder1);
        }

        #[test]
        fn encoder_1_with_acceleration_counter_clockwise() {
            assert_eq!(guess(&[127, 126, 126, 127, 127]), Encoder1);
        }

        #[test]
        fn encoder_1_with_more_acceleration() {
            assert_eq!(guess(&[1, 2, 5, 5, 2]), Encoder1);
        }

        #[test]
        fn encoder_1_with_more_acceleration_counter_clockwise() {
            assert_eq!(guess(&[127, 126, 122, 122, 126]), Encoder1);
        }

        #[test]
        fn encoder_2_with_acceleration() {
            assert_eq!(guess(&[65, 66, 66, 65, 65]), Encoder2);
        }

        #[test]
        fn encoder_2_with_acceleration_counter_clockwise() {
            assert_eq!(guess(&[63, 62, 62, 63, 63]), Encoder2);
        }

        #[test]
        fn encoder_2_with_more_acceleration() {
            assert_eq!(guess(&[65, 66, 68, 68, 66]), Encoder2);
        }

        #[test]
        fn encoder_2_with_more_acceleration_counter_clockwise() {
            assert_eq!(guess(&[63, 62, 59, 59, 62]), Encoder2);
        }

        #[test]
        fn absolute_encoder_hitting_upper_boundary() {
            assert_eq!(guess(&[127, 127, 127, 127, 127]), Range);
            assert_eq!(guess(&[125, 126, 127, 127, 127]), Range);
        }

        #[test]
        fn absolute_encoder_hitting_lower_boundary_counter_clockwise() {
            assert_eq!(guess(&[0, 0, 0, 0, 0]), Range);
            assert_eq!(guess(&[2, 1, 0, 0, 0]), Range);
        }

        #[test]
        fn lower_range_with_duplicate_elements() {
            assert_eq!(guess(&[0, 0, 1, 1, 2, 2]), Range);
        }

        #[test]
        fn lower_range_with_duplicate_elements_counter_clockwise() {
            assert_eq!(guess(&[2, 2, 1, 1, 0, 0]), Range);
        }

        #[test]
        fn neutral_zone_range_with_duplicate_elements() {
            assert_eq!(guess(&[37, 37, 37, 38, 38, 38, 39, 39]), Range);
        }

        #[test]
        fn neutral_zone_range_with_duplicate_elements_counter_clockwise() {
            assert_eq!(guess(&[100, 100, 100, 99, 99, 99, 98, 98]), Range);
        }

        fn guess(values: &[u8]) -> SourceCharacter {
            let u7_values: Vec<_> = values.into_iter().map(|v| u7(*v)).collect();
            guess_custom_character(&u7_values)
        }
    }
}
