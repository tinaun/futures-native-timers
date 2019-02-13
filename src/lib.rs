#![feature(futures_api, async_await, await_macro)]


use std::time::Duration;
use std::sync::{Arc, Mutex};

use std::pin::Pin;

use futures::prelude::*;
use futures::future::FusedFuture;
use futures::task::{Poll, LocalWaker, Waker};

#[macro_export]
macro_rules! dbg_println {
    ($($t:tt)*) => {
        #[cfg(test)]
        {
            println!($($t)*);
        }
    };
}

#[path = "sys/windows.rs"]
mod imp;
use imp::Timer;

#[derive(Debug)]
pub (crate) struct TimerState {
    wake: Option<Waker>,
    done: bool,
}

impl TimerState {
    fn new() -> Self {
        TimerState {
            wake: None,
            done: false,
        }
    }

    fn register_waker(&mut self, lw: &LocalWaker) {
        self.wake = Some(lw.as_waker().clone());
    }
}


#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Delay {
    handle: Timer,
    inner: Arc<Mutex<TimerState>>,
    delay: Duration,
    done: bool,
}

impl Delay {
    pub fn new(delay: Duration) -> Self {
        let state = Arc::new(Mutex::new(TimerState::new()));

        unsafe {
            let ptr = Arc::into_raw(state);
            let handle = Timer::new(ptr as *mut _);
            let inner = Arc::from_raw(ptr);


            Delay {
                handle,
                inner,
                delay,
                done: false,
            }
        }
    }

    fn register_waker(&mut self, lw: &LocalWaker) {
        let mut state = self.inner.lock().unwrap();
        state.register_waker(lw);
        
        
        if !self.handle.is_active() {
            self.handle.init_delay(self.delay);
        }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Self::Output> {
        self.register_waker(lw);
        if self.inner.lock().unwrap().done {
            self.done = true;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl FusedFuture for Delay {
    fn is_terminated(&self) -> bool {
        self.done
    }
}

impl Unpin for Delay {}


#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use std::time::{Duration, Instant};
    use super::*;

    #[test]
    fn join_timers() {
        use futures::join;

        let short = Delay::new(Duration::from_secs(1));
        let long = Delay::new(Duration::from_secs(3));

        let work = async {
            let t = Instant::now();
            let _ = join!(short, long);
            t.elapsed().as_secs()
        };

        let res = block_on(work);
        assert!(res < 4);
    }

    #[test]
    fn select_timers() {
        use futures::select;

        let mut short = Delay::new(Duration::from_secs(1));
        let mut long = Delay::new(Duration::from_secs(3));

        let work = async {
            select! {
                _ = short => "short finished first",
                _ = long => "long finished first",
            }
        };

        let res = block_on(work);
        assert_eq!(res, "short finished first");
    }
}

