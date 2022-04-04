use crate::domain::{DomainEventHandler, KeyMessage, Keystroke, SharedMainProcessors};
use reaper_medium::{TranslateAccel, TranslateAccelArgs, TranslateAccelResult};

#[derive(Debug)]
pub struct RealearnAccelerator<EH: DomainEventHandler> {
    main_processors: SharedMainProcessors<EH>,
}

impl<EH: DomainEventHandler> RealearnAccelerator<EH> {
    pub fn new(main_processors: SharedMainProcessors<EH>) -> Self {
        Self { main_processors }
    }
}

impl<EH: DomainEventHandler> RealearnAccelerator<EH> {
    fn process_message(&mut self, msg: KeyMessage) -> TranslateAccelResult {
        let mut filter_out_event = false;
        for proc in &mut *self.main_processors.borrow_mut() {
            if proc.wants_keys() && proc.process_incoming_key_msg(msg) {
                filter_out_event = true;
            }
        }
        if filter_out_event {
            TranslateAccelResult::Eat
        } else {
            TranslateAccelResult::NotOurWindow
        }
    }
}

impl<EH: DomainEventHandler> TranslateAccel for RealearnAccelerator<EH> {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        let stroke = Keystroke::normalized(args.msg.behavior, args.msg.key);
        let msg = KeyMessage::new(args.msg.message, stroke);
        self.process_message(msg)
    }
}
