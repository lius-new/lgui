use windows::{
    core::PCWSTR,
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

pub(crate) fn open_external_url(url: &str) -> windows::core::Result<()> {
    let operation = wide_null("open");
    let url = wide_null(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(windows::core::Error::from_thread())
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
