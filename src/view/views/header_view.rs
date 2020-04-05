use crate::model::RealearnSession;
use crate::view::bindings::root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX;
use crate::view::{View, Window};
use c_str_macro::c_str;
use helgoboss_midi::channel;
use reaper_rs::high_level::Reaper;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct HeaderView<'a> {
    session: Rc<RefCell<RealearnSession<'a>>>,
    window: Option<Window>,
}

impl<'a> HeaderView<'a> {
    pub fn new(session: Rc<RefCell<RealearnSession<'a>>>) -> HeaderView<'a> {
        HeaderView {
            session,
            window: None,
        }
    }
}

impl<'a> View for HeaderView<'a> {
    fn opened(&mut self, window: Window) {
        self.window = Some(window);
        Reaper::get().show_console_msg(c_str!("Opened header view\n"));
        self.session
            .borrow_mut()
            .get_dummy_source_model()
            .changed()
            .subscribe(move |_| {
                // TODO Think about the following
                // Take a weak ptr to self somehow or any other thing that can go away but we
                // need here. A very interesting idea is to make all methods take self as Rc<Self>.
                // That would of course require to hold views always as Rc. The question is, should
                // we do that?
                //
                // a) I mean, there are certainly ways to do everything without capturing self in
                // the closure. We could just take members of self (copyable or Rc) and work with
                // them only. The advantage being that this is more fine-grained so
                // it's less likely to break the (runtime RefCell) borrow checker rules. And we can
                // hold views any way we like. Plus, we don't need to borrow the
                // RefCell every time we want to access self.
                //
                // b) However, I don't know if at some point this gets old and we just wish we would
                // have access to self in the closures. This is a weak argument though. A more
                // important one might come up if we think about the question if we shouldn't hold
                // views as Rc (with RefCell) anyway ... for safety reasons. After all we actually
                // hold 2 references to each view *already*: The owning view holds a reference and
                // the win32 system holds one as well. The second one Rust compiler doesn't know
                // about because we use unsafe blocks. The first one is the "primary" reference.
                // The second one acts more like a weak pointer, only that we need to make sure
                // ourselves that win32 doesn't call anymore if the view is gone. Even if we get
                // this right, this can still cause problems. win32 window procedures can be
                // *reentered*, see the win32 docs! If we would have a borrow checker
                // (RefCell), it would complain in that case, big time, because (at least currently)
                // we let the window procedure call &mut self methods of the view. But we don't use
                // RefCell right now, so there's no borrow checker. What does that mean? It means
                // what we currently do is very unsafe and not even a runtime check makes us aware
                // of it. Moreover, who knows, some weird reentrancy situation might even cause
                // Rust to drop our view too early because it doesn't know about the second
                // reference. So it would certainly be much more correct and safe to protect each
                // view access using a Rc<RefCell<...>>. Then Rust knows about each reference and
                // can complain about non-exclusive mutable accesses.However, there's no point in
                // doing that if reentrancy and therefore non-exclusive mutable access can and will
                // happen anyway! We would get panics all over the place and wouldn't be able to do
                // anything about it because it's normal win32 behavior.
                //
                // c) So neither our current way (a) nor b is fine. I think the only correct
                // way is to never let the window procedure call view methods in a mutable context.
                // Make all view handler methods take an immutable reference. The same strategy
                // which we are using with IReaperControlSurface in reaper-rs, because this is
                // reentrant as well. Then we would need a RefCell in our view anyway for everything
                // that we want to be mutable. We would have to pursue the fine-granular RefCells
                // way because reentrancy is unavoidable. We just need to make sure not to write to
                // the same view sub data non-exclusively, which we could manage. Concerning the
                // potential premature drop issue: We should definitely use an Rc because under the
                // covers we really have multiple references. One by the owner and one which comes
                // and goes with each window procedure call. Let's make that safe!
                // About the question if it makes sense to take self as Rc<Self> ... it could. At
                // least we would have an easy way to access view methods in subscribe handlers.
                // We don't take self as Rc<RefCell<Self>>, so there's no acute danger of
                // non-exclusive mutable access just by accessing the view. We can easily try that
                // strategy soon because the plan is to hold each view as Rc anyway.
                println!("Dummy source model changed");
                window
                    .find_control(ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
                    .unwrap()
                    .set_text("test");
            });
    }

    fn closed(&mut self) {
        self.window = None;
    }

    fn button_clicked(&mut self, resource_id: u32) {
        Reaper::get().show_console_msg(c_str!("Clicked button\n"));
        self.session
            .borrow_mut()
            .get_dummy_source_model()
            .channel
            .set(Some(channel(14)));
    }
}
