//! 急ぐ子に 先を譲りて 道を空く

pub fn promote() {
    if let Err(error) = imp::promote() {
        log::debug!("audio thread keeps its normal priority: {error}");
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::c_void;

    #[link(name = "avrt")]
    unsafe extern "system" {
        fn AvSetMmThreadCharacteristicsW(
            task_name: *const u16,
            task_index: *mut u32,
        ) -> *mut c_void;
    }

    // 楽の間に 席を設けて 待たせざる
    pub fn promote() -> Result<(), String> {
        let task = "Pro Audio".encode_utf16().chain([0]).collect::<Vec<u16>>();
        let mut index = 0u32;
        let handle = unsafe { AvSetMmThreadCharacteristicsW(task.as_ptr(), &mut index) };
        if handle.is_null() {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
    }

    pub fn promote() -> Result<(), String> {
        match unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) } {
            0 => Ok(()),
            code => Err(std::io::Error::from_raw_os_error(code).to_string()),
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::{c_int, c_ulong};

    const SCHED_RR: c_int = 2;

    #[repr(C)]
    struct SchedParam {
        priority: c_int,
    }

    unsafe extern "C" {
        fn pthread_self() -> c_ulong;
        fn pthread_setschedparam(thread: c_ulong, policy: c_int, param: *const SchedParam)
        -> c_int;
    }

    pub fn promote() -> Result<(), String> {
        let param = SchedParam { priority: 10 };
        match unsafe { pthread_setschedparam(pthread_self(), SCHED_RR, &param) } {
            0 => Ok(()),
            code => Err(std::io::Error::from_raw_os_error(code).to_string()),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod imp {
    pub fn promote() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn audio_threads_can_be_promoted() {
        let promoted = std::thread::spawn(super::imp::promote).join().unwrap();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert!(promoted.is_ok(), "{promoted:?}");
        drop(promoted);
    }
}
