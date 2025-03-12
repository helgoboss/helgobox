use crate::base::*;
use crate::ext::ok_button;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    Dialog {
        id: ids.named_id("ID_SETUP_PANEL"),
        optional: true,
        caption: "Welcome to Helgobox!",
        rect: context.rect(0, 0, 250, 280),
        styles: Styles(vec![
            // Places the window into the center by default
            DS_CENTER, // Displays a close button
            WS_SYSMENU,
        ]),
        controls: vec![
            ctext(
                "Intro text 1",
                ids.named_id("ID_SETUP_INTRO_TEXT_1"),
                context.rect(25, 25, 200, 50),
            ),
            ctext(
                "Intro text 2",
                ids.named_id("ID_SETUP_INTRO_TEXT_2"),
                context.rect(25, 80, 200, 30),
            ),
            context.checkbox(
                "Add Playtime button to main toolbar",
                ids.named_id("ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON"),
                context.rect(60, 120, 150, 8),
            ),
            context.checkbox(
                "Send errors to developer automatically",
                ids.named_id("ID_SETUP_SEND_ERRORS_TO_DEV"),
                context.rect(60, 135, 150, 8),
            ),
            context.checkbox(
                "Show errors in console",
                ids.named_id("ID_SETUP_SHOW_ERRORS_IN_CONSOLE"),
                context.rect(60, 150, 150, 8),
            ),
            context.checkbox(
                "Notify about updates",
                ids.named_id("ID_SETUP_NOTIFY_ABOUT_UPDATES"),
                context.rect(60, 165, 150, 8),
            ),
            ctext(
                "Comment",
                ids.named_id("ID_SETUP_COMMENT"),
                context.rect(25, 185, 200, 25),
            ),
            ctext(
                "Tip",
                ids.named_id("ID_SETUP_TIP_TEXT"),
                context.rect(25, 210, 200, 25),
            ),
            ok_button(
                ids.named_id("ID_SETUP_PANEL_OK"),
                context.rect(75, 240, 100, 14),
            ),
        ],
        ..context.default_dialog()
    }
}
