use reaper_rx::{ActionRx, ActionRxProvider, ControlSurfaceRx, MainRx};
use std::cell::RefCell;

make_available_globally_in_main_thread!(Global);

#[derive(Default)]
pub struct Global {
    main_rx: MainRx,
}

impl Global {
    // This is kept static just for allowing easy observable subscription from everywhere. For
    // pushing to the subjects, static access is not necessary.
    pub fn control_surface_rx() -> &'static ControlSurfaceRx {
        Global::get().main_rx.control_surface()
    }

    // This really needs to be kept static for pushing to the subjects because hook commands can't
    // take user data.
    pub fn action_rx() -> &'static ActionRx {
        Global::get().main_rx.action()
    }
}

impl ActionRxProvider for Global {
    fn action_rx() -> &'static ActionRx {
        Global::action_rx()
    }
}
