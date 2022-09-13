use crate::domain::{
    FeedbackOutput, FinalRealFeedbackValue, FinalSourceFeedbackValue, MidiDestination,
    PreliminaryRealFeedbackValue, PreliminarySourceFeedbackValue, RealearnSourceState,
};
use helgoboss_learn::devices::x_touch::XTouchMackieLcdState;
use helgoboss_learn::{
    MackieLcdScope, MidiSourceValue, PreliminaryMidiSourceFeedbackValue, RawMidiEvent,
    XTouchMackieLcdColorRequest,
};
use std::collections::HashSet;

/// Responsible for collecting non-final feedback values and aggregating them into final ones.
pub struct FeedbackCollector<'a> {
    x_touch_mackie_lcd_feedback_collector: Option<XTouchMackieLcdFeedbackCollector<'a>>,
}

struct XTouchMackieLcdFeedbackCollector<'a> {
    state: &'a mut XTouchMackieLcdState,
    changed_x_touch_mackie_lcd_extenders: HashSet<u8>,
}

impl<'a> FeedbackCollector<'a> {
    pub fn new(
        global_source_state: &'a mut RealearnSourceState,
        feedback_output: Option<FeedbackOutput>,
    ) -> Self {
        let x_touch_mackie_lcd_state = match feedback_output {
            Some(FeedbackOutput::Midi(MidiDestination::Device(dev_id))) => {
                Some(global_source_state.get_x_touch_mackie_lcd_state_mut(dev_id))
            }
            // No or no direct MIDI device output. Then we can ignore this because
            // the X-Touch!
            _ => None,
        };
        Self {
            x_touch_mackie_lcd_feedback_collector: x_touch_mackie_lcd_state.map(|state| {
                XTouchMackieLcdFeedbackCollector {
                    state,
                    changed_x_touch_mackie_lcd_extenders: Default::default(),
                }
            }),
        }
    }

    pub fn process(
        &mut self,
        preliminary_feedback_value: PreliminaryRealFeedbackValue,
    ) -> Option<FinalRealFeedbackValue> {
        match preliminary_feedback_value.source {
            // Has projection part only.
            None => FinalRealFeedbackValue::new(preliminary_feedback_value.projection, None),
            Some(preliminary_source_feedback_value) => match preliminary_source_feedback_value {
                PreliminarySourceFeedbackValue::Midi(v) => match v {
                    // Is final MIDI value already.
                    PreliminaryMidiSourceFeedbackValue::Final(v) => FinalRealFeedbackValue::new(
                        preliminary_feedback_value.projection,
                        Some(FinalSourceFeedbackValue::Midi(v)),
                    ),
                    // Is non-final.
                    PreliminaryMidiSourceFeedbackValue::XTouchMackieLcdColor(req) => {
                        self.process_x_touch_mackie_lcd_color_request(req);
                        None
                    }
                },
                // Is final OSC value already.
                PreliminarySourceFeedbackValue::Osc(v) => FinalRealFeedbackValue::new(
                    preliminary_feedback_value.projection,
                    Some(FinalSourceFeedbackValue::Osc(v)),
                ),
            },
        }
    }

    /// Takes the collected and aggregated material and produces the final feedback values.
    pub fn generate_final_feedback_values(self) -> impl Iterator<Item = FinalRealFeedbackValue> {
        self.x_touch_mackie_lcd_feedback_collector
            .into_iter()
            .flat_map(|x_touch_collector| {
                x_touch_collector
                    .changed_x_touch_mackie_lcd_extenders
                    .into_iter()
                    .filter_map(|extender_index| {
                        let sysex = x_touch_collector.state.sysex(extender_index);
                        let midi_event = RawMidiEvent::try_from_iter(0, sysex).ok()?;
                        let source_feedback_value = FinalSourceFeedbackValue::Midi(
                            MidiSourceValue::single_raw(None, midi_event),
                        );
                        FinalRealFeedbackValue::new(None, Some(source_feedback_value))
                    })
            })
    }

    fn process_x_touch_mackie_lcd_color_request(&mut self, req: XTouchMackieLcdColorRequest) {
        let collector = match &mut self.x_touch_mackie_lcd_feedback_collector {
            None => return,
            Some(c) => c,
        };
        let channels = match req.channel {
            None => (0..MackieLcdScope::CHANNEL_COUNT),
            Some(ch) => (ch..ch + 1),
        };
        let mut at_least_one_color_change = false;
        for ch in channels {
            let changed =
                collector
                    .state
                    .notify_color_requested(req.extender_index, ch, req.color_index);
            if changed {
                at_least_one_color_change = true;
            }
        }
        if at_least_one_color_change {
            collector
                .changed_x_touch_mackie_lcd_extenders
                .insert(req.extender_index);
        }
    }
}
