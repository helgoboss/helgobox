use backtrace::Symbol;
use clipboard::{ClipboardContext, ClipboardProvider};
use reaper_high::{ActionKind, Reaper};
use regex::Regex;

pub fn register_resolve_symbols_action() {
    Reaper::get().register_action(
        "REALEARN_RESOLVE_SYMBOLS",
        "[developer] ReaLearn: Resolve symbols from clipboard",
        || {
            if let Err(e) = resolve_symbols_from_clipboard() {
                Reaper::get().show_console_msg(format!("{}\n", e));
            }
        },
        ActionKind::NotToggleable,
    );
}

fn resolve_symbols_from_clipboard() -> Result<(), &'static str> {
    let mut clipboard: ClipboardContext =
        ClipboardProvider::new().map_err(|_| "Couldn't obtain clipboard.")?;
    let text = clipboard
        .get_contents()
        .map_err(|_| "Couldn't read from clipboard.")?;
    resolve_symbols_from_text(&text)
}

fn resolve_symbols_from_text(text: &str) -> Result<(), &'static str> {
    // There was no "0x" prefix. Parse all addresses from text.
    let regex = Regex::new(r" 0x[0-9a-f]+ ")
        .map_err(|_| "Couldn't find any addresses (e.g. 0x7ffac481cec1) in text.")?;
    let address_strings = regex
        .find_iter(text)
        .map(|m| m.as_str().trim().trim_start_matches("0x"));
    let addresses: Result<Vec<isize>, _> = address_strings
        .map(|s| isize::from_str_radix(s, 16).map_err(|_| "invalid address"))
        .collect();
    resolve_multiple_symbols(&addresses?);
    Ok(())
}

fn resolve_multiple_symbols(addresses: &Vec<isize>) {
    Reaper::get().show_console_msg(format!(
        "Attempting to resolve symbols for {} addresses...\n\n",
        addresses.len()
    ));
    for a in addresses {
        resolve_one_of_multiple_symbols(*a);
    }
}

fn resolve_one_of_multiple_symbols(address: isize) {
    backtrace::resolve(address as _, |sym| {
        Reaper::get().show_console_msg(format!("{}\n\n", format_symbol_terse(sym)));
    });
}

fn format_symbol_terse(sym: &Symbol) -> String {
    let segments: Vec<String> = vec![
        sym.addr().map(|a| format!("{:x?}", a as isize)),
        sym.name().map(|n| n.to_string()),
        sym.filename().map(|p| {
            format!(
                "{}{}",
                p.to_string_lossy(),
                sym.lineno()
                    .map(|n| format!(" (line {})", n))
                    .unwrap_or_else(|| "".to_string())
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    segments.join("\n")
}
