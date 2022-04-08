// This file handles dialog generation. This is necessary on Linux and Mac OS X to make Swell::CreateDialogParam() work
// with our UI created in ResEdit. This is still done using C++ because that mechanism is weird. There's no point
// in investing much effort into porting this to Rust. It will not offer any big benefit.

// We want to use the SWELL functions offered by REAPER instead of compiling SWELL into our plug-in.
#define SWELL_PROVIDED_BY_APP

// Some preparation for dialog generation.
#include "msvc/Resource.h"
#include "../../../lib/WDL/WDL/swell/swell.h"
// Make sure the following factors correspond to the ones in `units.rs` (function `effective_scale_factors`).
#ifdef __APPLE__
#define SWELL_DLG_SCALE_AUTOGEN 1.6
#define SWELL_DLG_SCALE_AUTOGEN_YADJ 0.90
#else
#define SWELL_DLG_SCALE_AUTOGEN 1.9
#define SWELL_DLG_SCALE_AUTOGEN_YADJ 1.0
#endif
#include "../../../lib/WDL/WDL/swell/swell-dlggen.h"
#define CBS_HASSTRINGS 0
#define WS_EX_LEFT
#define WC_COMBOBOX "ComboBox"
#define SS_WORDELLIPSIS 0

// This is the result of the dialog RC file conversion (via PHP script).
#include "realearn.rc_mac_dlg"

// Now let's take care of menus
#include "../../../lib/WDL/WDL/swell/swell-menugen.h"
#include "realearn.rc_mac_menu"