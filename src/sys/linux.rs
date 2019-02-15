#![allow(non_camel_case_types)]

use std::ptr;
use std::mem;
use std::sync::{Once, Mutex};
use std::time::Duration;
use super::TimerState;

use libc::{
    c_int,
    c_void,
    clockid_t,
    CLOCK_MONOTONIC,
    itimerspec,
    sigaction,
    siginfo_t,
    sigevent,
    timespec,
};

// for some reason these aren't in the libc crate yet.

type timer_t = usize;

extern {
    fn timer_create(clockid: clockid_t, sevp: *mut sigevent, timerid: *mut timer_t) -> c_int;
    fn timer_settime(timerid: timer_t, flags: c_int, new_value: *const itimerspec, old_value: *mut itimerspec) -> c_int;
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
    let raw_bytes = (*si)._pad;
    let val: libc::sigval = mem::transmute([raw_bytes[3], raw_bytes[4]]);
    
    
    let context = val.sival_ptr as *mut Mutex<TimerState>;
    dbg_println!("handled - {:p}", context);

    let mut state = (*context).lock().unwrap();
    
    state.done = true;
    state.wake.as_ref().map(|w| w.wake());
    
}




#[derive(Debug)]
pub struct NativeTimer {
    inner: timer_t,
    active: bool,
    _no_send: *const (),
}

impl NativeTimer {
    pub(crate) unsafe fn new( state: *mut Mutex<TimerState> ) -> Self {
        HANDLER.call_once(|| init_handler());
        dbg_println!("{:p}", state);

        let sival_ptr = state as *mut _ ;
        let mut sev: sigevent = mem::zeroed();
        sev.sigev_value = libc::sigval { sival_ptr };
        sev.sigev_signo = MYSIG;
        sev.sigev_notify = libc::SIGEV_THREAD_ID;
        let tid = libc::syscall(libc::SYS_gettid);
        sev.sigev_notify_thread_id = tid as i32;


        let mut timer = 0;
        let res = timer_create(CLOCK_MONOTONIC, &mut sev, &mut timer);
        assert_eq!(res, 0);

        NativeTimer {
            inner: timer,
            active: false,
            _no_send: ptr::null(),   
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn init_delay(&mut self, delay: Duration) {
        let ticks = timespec {
            tv_sec: delay.as_secs() as i64,
            tv_nsec: delay.subsec_nanos() as i64,
        };

        self.init(ticks, None);
    }

    pub fn init_interval(&mut self, interval: Duration) {
        let ticks = timespec {
            tv_sec: interval.as_secs() as i64,
            tv_nsec: interval.subsec_nanos() as i64,
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
