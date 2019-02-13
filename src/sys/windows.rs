use std::ptr;
use std::time::Duration;
use std::sync::Mutex;
use super::{TimerState, dbg_println};

use winapi::shared::minwindef::{TRUE, FILETIME};
use winapi::um::winnt::{
    PTP_CALLBACK_INSTANCE,
    PVOID,
    PTP_TIMER,
};

use winapi::um::threadpoolapiset::{
    CreateThreadpoolTimer,
    SetThreadpoolTimerEx,
    WaitForThreadpoolTimerCallbacks,
    CloseThreadpoolTimer,
};

unsafe extern "system" fn timer_callback(_instance: PTP_CALLBACK_INSTANCE, context: PVOID, _timer: PTP_TIMER) {

    let context = context as *mut Mutex<TimerState>;
    let mut state = (*context).lock().unwrap();
    state.done = true;
    state.wake.as_ref().unwrap().wake();

}

#[derive(Debug)]
pub struct Timer {
    inner: PTP_TIMER,
    active: bool,
}

impl Timer {
    pub(crate) unsafe fn new( state: *mut Mutex<TimerState> ) -> Self {
        let timer = CreateThreadpoolTimer(Some(timer_callback), state as *mut _ , ptr::null_mut());

        Timer {
            inner: timer,
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active    
    }

    pub fn init_delay(&mut self, delay: Duration) {
        self.active = true;
        dbg_println!("timer started!");
        let mut ticks = (delay.subsec_nanos() / 100) as i64;
        ticks += (delay.as_secs() * 10_000_000) as i64;
        let ticks = -ticks;

        unsafe {
            // i need to find a better way to do this. :/
            // probably byteorder? windows apis are super weird - where else would a i64 
            // have to be represented as two u32s
            let mut time: FILETIME = std::mem::transmute(ticks);
            SetThreadpoolTimerEx(self.inner, &mut time, 0, 0);
        }
        
    } 
}

impl Drop for Timer {
    fn drop(&mut self) {
        unsafe {
            SetThreadpoolTimerEx(self.inner, ptr::null_mut(), 0, 0);
            WaitForThreadpoolTimerCallbacks(self.inner, TRUE);
            CloseThreadpoolTimer(self.inner);
        }

        dbg_println!("timer closed.");
    }
}
