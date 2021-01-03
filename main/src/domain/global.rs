use reaper_rx::{ActionRx, ActionRxProvider, ControlSurfaceRx, MainRx};

make_available_globally_in_main_thread!(Global);

#[derive(Default)]
pub struct Global {
    main_rx: MainRx,
}

impl Global {
    pub fn control_surface_rx() -> &'static ControlSurfaceRx {
        Global::get().main_rx.control_surface()
    }

    pub fn action_rx() -> &'static ActionRx {
        Global::get().main_rx.action()
    }
}

impl ActionRxProvider for Global {
    fn action_rx() -> &'static ActionRx {
        Global::action_rx()
    }
}
