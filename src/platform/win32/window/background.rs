pub(super) fn trim_process_working_set() {
    unsafe {
        use windows::Win32::System::{
            ProcessStatus::EmptyWorkingSet,
            Threading::{GetCurrentProcess, SetProcessWorkingSetSize},
        };

        let process = GetCurrentProcess();
        if EmptyWorkingSet(process).is_err() {
            let _ = SetProcessWorkingSetSize(process, usize::MAX, usize::MAX);
        }
    }
}
