#![allow(non_camel_case_types)]

use super::TimerState;
use std::mem;
use std::ptr;
use std::sync::Once;
use std::time::Duration;

use libc::{
    c_int, c_void, clockid_t, itimerspec, sigaction, sigevent, siginfo_t, suseconds_t, time_t,
    timespec, CLOCK_MONOTONIC,
};

// for some reason these aren't in the libc crate yet.

type timer_t = usize;

extern "C" {
    fn timer_create(clockid: clockid_t, sevp: *mut sigevent, timerid: *mut timer_t) -> c_int;
    fn timer_settime(
        timerid: timer_t,
        flags: c_int,
        new_value: *const itimerspec,
        old_value: *mut itimerspec,
    ) -> c_int;
    fn timer_delete(timerid: timer_t);
}

// set up the signal handler
// FIXME: find a free real-time signal properly
static HANDLER: Once = Once::new();
const MYSIG: c_int = 40;

unsafe fn init_handler() {
    let mut sa: sigaction = mem::zeroed();
    sa.sa_flags = libc::SA_SIGINFO;
    sa.sa_sigaction = handler as usize;
    libc::sigemptyset(&mut sa.sa_mask);

    if sigaction(MYSIG, &sa, ptr::null_mut()) == -1 {
        panic!("error creating timer sigal handler!");
    }
}

unsafe extern "C" fn handler(_sig: c_int, si: *mut siginfo_t, _uc: *mut c_void) {
    // evil things are afoot - tread wisely.
    //
    // the `libc` crate exposes the union part of siginfo_t as a array of i32s,
    // so we have to manually track the offset to get the correct field.
    //
    let raw_bytes = (*si)._pad;
    let val: libc::sigval = ptr::read(raw_bytes[3..].as_ptr() as *const _);

    let state = val.sival_ptr as *mut TimerState;
    dbg_println!("handled - {:p}", state);

    (*state).set_done(true);
    (*state).wake.wake();
}

#[derive(Debug)]
pub struct NativeTimer {
    inner: timer_t,
    active: bool,
}

impl NativeTimer {
    pub(crate) unsafe fn new(state: *mut TimerState) -> Self {
        HANDLER.call_once(|| init_handler());
        dbg_println!("{:p}", state);

        let sival_ptr = state as *mut _;
        let mut sev: sigevent = mem::zeroed();
        sev.sigev_value = libc::sigval { sival_ptr };
        sev.sigev_signo = MYSIG;

        // yes, this means that if you create a timer on a thread that later is dropped,
        // timer events won't fire. changing this to SIGEV_SIGNAL leads to complete
        // non-deterministic behavior when running tests, since any thread could be
        // interupted for any signal.
        //
        // this is unfortunate, but will do for now - generally futures executors don't
        // tend to kill and respawn threads often.
        //
        // a solution to this might have to involve a dedicated thread for signal handling.
        sev.sigev_notify = libc::SIGEV_THREAD_ID;
        let tid = libc::syscall(libc::SYS_gettid);
        sev.sigev_notify_thread_id = tid as i32;

        let mut timer = 0;
        let res = timer_create(CLOCK_MONOTONIC, &mut sev, &mut timer);
        assert_eq!(res, 0);

        NativeTimer {
            inner: timer,
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn init_delay(&mut self, delay: Duration) {
        let ticks = timespec {
            tv_sec: delay.as_secs() as time_t,
            tv_nsec: delay.subsec_nanos() as suseconds_t,
        };

        self.init(ticks, None);
    }

    pub fn init_interval(&mut self, interval: Duration) {
        let ticks = timespec {
            tv_sec: interval.as_secs() as time_t,
            tv_nsec: interval.subsec_nanos() as suseconds_t,
        };

        self.init(ticks, Some(ticks));
    }

    fn init(&mut self, start: timespec, repeat: Option<timespec>) {
        dbg_println!("created timer!");
        self.active = true;

        let repeat = repeat.unwrap_or(timespec {
            tv_sec: 0,
            tv_nsec: 0,
        });

        let new_value = itimerspec {
            it_interval: repeat,
            it_value: start,
        };

        unsafe {
            let res = timer_settime(self.inner, 0, &new_value, ptr::null_mut());
            assert_eq!(res, 0);
        }
    }
}

impl Drop for NativeTimer {
    fn drop(&mut self) {
        unsafe {
            timer_delete(self.inner);
        }
    }
}
