use super::{dbg_println, TimerState};
use std::ptr;
use std::time::Duration;

use winapi::shared::minwindef::{FILETIME, TRUE};
use winapi::um::winnt::{PTP_CALLBACK_INSTANCE, PTP_TIMER, PVOID};

use winapi::um::threadpoolapiset::{
    CloseThreadpoolTimer, CreateThreadpoolTimer, SetThreadpoolTimerEx,
    WaitForThreadpoolTimerCallbacks,
};

unsafe extern "system" fn timer_callback(
    _instance: PTP_CALLBACK_INSTANCE,
    context: PVOID,
    _timer: PTP_TIMER,
) {
    let state = context as *mut TimerState;

    (*state).set_done(true);
    (*state).wake.wake();
}

#[derive(Debug)]
pub struct NativeTimer {
    inner: PTP_TIMER,
    active: bool,
}

impl NativeTimer {
    pub(crate) unsafe fn new(state: *mut TimerState) -> Self {
        let timer = CreateThreadpoolTimer(Some(timer_callback), state as *mut _, ptr::null_mut());

        NativeTimer {
            inner: timer,
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn init_delay(&mut self, delay: Duration) {
        let mut ticks = (delay.subsec_nanos() / 100) as i64;
        ticks += (delay.as_secs() * 10_000_000) as i64;
        let ticks = -ticks;

        self.init(ticks, 0);
    }

    pub fn init_interval(&mut self, interval: Duration) {
        let mut ticks = (interval.subsec_nanos() / 100) as i64;
        ticks += (interval.as_secs() * 10_000_000) as i64;
        let millis = (ticks / 10_000) as u32;
        let ticks = -ticks;

        self.init(ticks, millis);
    }

    fn init(&mut self, start: i64, repeat: u32) {
        self.active = true;
        dbg_println!("timer started!");

        unsafe {
            // i need to find a better way to do this. :/
            // probably byteorder? windows apis are super weird - where else would a i64
            // have to be represented as two u32s
            let mut time: FILETIME = std::mem::transmute(start);
            SetThreadpoolTimerEx(self.inner, &mut time, repeat, 0);
        }
    }
}

impl Drop for NativeTimer {
    fn drop(&mut self) {
        unsafe {
            SetThreadpoolTimerEx(self.inner, ptr::null_mut(), 0, 0);
            WaitForThreadpoolTimerCallbacks(self.inner, TRUE);
            CloseThreadpoolTimer(self.inner);
        }

        dbg_println!("timer closed.");
    }
}

unsafe impl Send for NativeTimer {}
unsafe impl Sync for NativeTimer {}
