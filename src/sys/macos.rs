#![allow(non_camel_case_types)]

use super::TimerState;
use std::time::Duration;

use libc::{c_long, c_ulong, c_void, int64_t, uint64_t, uintptr_t};

type dispatch_object_t = *const c_void;
type dispatch_queue_t = *const c_void;
type dispatch_source_t = *const c_void;
type dispatch_source_type_t = *const c_void;
type dispatch_time_t = uint64_t;

const DISPATCH_TIME_NOW: dispatch_time_t = 0;
const QOS_CLASS_DEFAULT: c_long = 0x15;

extern "C" {
    static _dispatch_source_type_timer: c_long;

    fn dispatch_get_global_queue(identifier: c_long, flags: c_ulong) -> dispatch_queue_t;
    fn dispatch_source_create(
        type_: dispatch_source_type_t,
        handle: uintptr_t,
        mask: c_ulong,
        queue: dispatch_queue_t,
    ) -> dispatch_source_t;
    fn dispatch_source_set_timer(
        source: dispatch_source_t,
        start: dispatch_time_t,
        interval: uint64_t,
        leeway: uint64_t,
    );
    fn dispatch_source_set_event_handler_f(
        source: dispatch_source_t,
        handler: unsafe extern "C" fn(*mut c_void),
    );
    fn dispatch_set_context(object: dispatch_object_t, context: *mut c_void);
    fn dispatch_resume(object: dispatch_object_t);
    fn dispatch_release(object: dispatch_object_t);
    fn dispatch_time(when: dispatch_time_t, delta: int64_t) -> dispatch_time_t;
}

#[derive(Debug)]
pub struct NativeTimer {
    timer: dispatch_source_t,
    active: bool,
}

unsafe impl Send for NativeTimer {}

impl NativeTimer {
    pub(crate) unsafe fn new(state: *mut TimerState) -> Self {
        let timer = dispatch_source_create(
            &_dispatch_source_type_timer as *const _ as dispatch_source_type_t,
            0, // handle (not used for timers)
            0, // mask (ditto)
            dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0),
        );

        dispatch_source_set_event_handler_f(timer, handler);
        dispatch_set_context(timer, state as *mut _);

        NativeTimer {
            timer,
            active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn init_delay(&mut self, delay: Duration) {
        unsafe {
            dispatch_source_set_timer(
                self.timer,
                dispatch_time(DISPATCH_TIME_NOW, delay.as_nanos() as int64_t),
                0, // interval
                0, // leeway
            );
            dispatch_resume(self.timer);
        }

        self.active = true;
    }

    pub fn init_interval(&mut self, interval: Duration) {
        unsafe {
            dispatch_source_set_timer(
                self.timer,
                dispatch_time(DISPATCH_TIME_NOW, interval.as_nanos() as int64_t),
                interval.as_nanos() as uint64_t,
                0, // leeway
            );
            dispatch_resume(self.timer);
        }

        self.active = true;
    }
}

impl Drop for NativeTimer {
    fn drop(&mut self) {
        unsafe {
            // The timer starts in a suspended state, and: "It is important to balance
            // calls to dispatch_suspend and dispatch_resume so that the dispatch object
            // is fully resumed when the last reference is released. The behavior when
            // releasing the last reference to a dispatch object while in a suspended
            // state is undefined."
            if !self.active {
                dispatch_resume(self.timer);
            }

            dispatch_release(self.timer);
        }
    }
}

unsafe extern "C" fn handler(context: *mut c_void) {
    let state = context as *mut TimerState;

    (*state).set_done(true)
    (*state).wake.wake();
}
