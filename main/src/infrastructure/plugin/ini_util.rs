use anyhow::bail;
use reaper_low::Swell;
use reaper_medium::ReaperStringArg;

pub fn write_ini_entry<'a>(
    file: impl Into<ReaperStringArg<'a>>,
    section: impl Into<ReaperStringArg<'a>>,
    key: impl Into<ReaperStringArg<'a>>,
    value: impl Into<ReaperStringArg<'a>>,
) -> anyhow::Result<()> {
    let success = unsafe {
        Swell::get().WritePrivateProfileString(
            section.into().as_ptr(),
            key.into().as_ptr(),
            value.into().as_ptr(),
            file.into().as_ptr(),
        )
    };
    if success < 1 {
        bail!("Couldn't write INI entry");
    }
    Ok(())
}
