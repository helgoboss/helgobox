use crate::domain::{DomainEventHandler, SharedMainProcessors};
use reaper_high::Reaper;
use reaper_medium::{
    Accel, AccelMsgKind, TranslateAccel, TranslateAccelArgs, TranslateAccelResult,
};

#[derive(Debug)]
pub struct RealearnAccelerator<EH: DomainEventHandler> {
    main_processors: SharedMainProcessors<EH>,
}

impl<EH: DomainEventHandler> RealearnAccelerator<EH> {
    pub fn new(main_processors: SharedMainProcessors<EH>) -> Self {
        Self { main_processors }
    }
}

impl<EH: DomainEventHandler> TranslateAccel for RealearnAccelerator<EH> {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        if args.msg.message != AccelMsgKind::KeyDown {
            return TranslateAccelResult::NotOurWindow;
        }
        let accel = Accel {
            f_virt: args.msg.behavior,
            key: args.msg.key,
            cmd: 0,
        };
        let reaper = Reaper::get().medium_reaper();
        let formatted = reaper.kbd_format_key_name(accel);
        tracing_debug!(
            "\
            Captured {:?}\n\
            Formatted: {},\n\
        ",
            &args,
            formatted
        );
        TranslateAccelResult::NotOurWindow
    }
}
