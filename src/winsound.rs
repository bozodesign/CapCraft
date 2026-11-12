//! Windows keeps a volume and a mute switch for every program (the Volume Mixer) and remembers
//! them for the program's file. When CapCraft is muted there, its playback is silent even though
//! the sound goes out, while an exported video still has sound (another program plays it).
//! This reads CapCraft's own entry and can switch it back on; it never touches other programs.

/// CapCraft's own entry in the Windows Volume Mixer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AppVolume {
    pub muted: bool,
    /// 0..1
    pub level: f32,
}

/// Below this the Volume Mixer leaves CapCraft too quiet to hear (our own choice); turning the
/// sound on brings it back to full.
pub const QUIET_LEVEL: f32 = 0.05;

impl AppVolume {
    pub fn silent(&self) -> bool {
        self.muted || self.level < QUIET_LEVEL
    }
}

#[cfg(windows)]
mod imp {
    use super::{AppVolume, QUIET_LEVEL};
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Media::Audio::{eConsole, eRender, IAudioSessionManager, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

    /// Runs `f` on CapCraft's own session of the default speakers, on a thread of its own so the
    /// window's thread keeps its own COM setup.
    fn with<T: Send + 'static>(f: impl FnOnce(&ISimpleAudioVolume) -> windows::core::Result<T> + Send + 'static) -> Option<T> {
        std::thread::spawn(move || unsafe {
            let inited = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let r = (|| {
                let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
                let mgr: IAudioSessionManager = dev.Activate(CLSCTX_ALL, None)?;
                // no session id and no flags: this program's own default session
                let v = mgr.GetSimpleAudioVolume(None, 0)?;
                f(&v)
            })();
            if inited {
                CoUninitialize();
            }
            r.ok()
        })
        .join()
        .ok()
        .flatten()
    }

    pub fn read() -> Option<AppVolume> {
        with(|v| unsafe { Ok(AppVolume { muted: v.GetMute()?.as_bool(), level: v.GetMasterVolume()? }) })
    }

    pub fn turn_on() -> bool {
        with(|v| unsafe {
            v.SetMute(BOOL(0), std::ptr::null())?;
            if v.GetMasterVolume()? < QUIET_LEVEL {
                v.SetMasterVolume(1.0, std::ptr::null())?;
            }
            Ok(())
        })
        .is_some()
    }
}

#[cfg(not(windows))]
mod imp {
    use super::AppVolume;
    pub fn read() -> Option<AppVolume> {
        None
    }
    pub fn turn_on() -> bool {
        false
    }
}

/// CapCraft's entry in the Volume Mixer, or None when it can't be read (or not on Windows).
pub fn read() -> Option<AppVolume> {
    imp::read()
}

/// Unmute CapCraft in the Volume Mixer (and bring a near-silent level back to full).
pub fn turn_on() -> bool {
    imp::turn_on()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_this_programs_volume_mixer_entry() {
        // the test program's own entry: readable on Windows, nothing elsewhere
        let v = super::read();
        if cfg!(windows) {
            let v = v.expect("Volume Mixer entry");
            assert!((0.0..=1.0).contains(&v.level));
        } else {
            assert!(v.is_none());
        }
    }
}
