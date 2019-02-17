#![feature(futures_api, async_await, await_macro)]


use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use std::pin::Pin;

use futures::prelude::*;
use futures::future::FusedFuture;
use futures::stream::FusedStream;
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

#[cfg(windows)]
#[path = "sys/windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "sys/linux.rs"]
mod imp;

use imp::NativeTimer;

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
struct Timer {
    handle: NativeTimer,
    state: Arc<Mutex<TimerState>>,
}

impl Timer {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(TimerState::new()));

        unsafe {
            let ptr = Arc::into_raw(state);
            let handle = NativeTimer::new(ptr as *mut _);
            let state = Arc::from_raw(ptr);


            Timer {
                handle,
                state,
            }
        }
    }

    fn register_waker(&mut self, lw: &LocalWaker) {
        let mut state = self.state.lock().unwrap();
        state.register_waker(lw);
    }

    fn is_active(&self) -> bool {
        self.handle.is_active()
    }

    fn is_done(&self) -> bool {
        self.state.lock().unwrap().done
    }
}


#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Delay {
    inner: Timer,
    delay: Duration,
    done: bool,
}

impl Delay {
    pub fn new(delay: Duration) -> Self {
        let inner = Timer::new();

        Delay {
            inner,
            delay,
            done: false,
        }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Self::Output> {
        if !self.inner.is_active() {
            let delay = self.delay;
            self.inner.handle.init_delay(delay);
        }

        self.inner.register_waker(lw);
        if self.inner.is_done() {
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


pub struct Interval {
    inner: Timer,
    interval: Duration,
}

impl Interval {
    pub fn new(interval: Duration) -> Self {
        let inner = Timer::new();

        Interval {
            inner,
            interval,
        }
    }
}

impl Stream for Interval {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Option<Self::Item>> {
        if !self.inner.is_active() {
            let interval = self.interval;
            self.inner.handle.init_interval(interval);
        }

        self.inner.register_waker(lw);
        let mut state = self.inner.state.lock().unwrap();
        if state.done {
            state.done = false;
            Poll::Ready(Some(Instant::now()))
        } else {
            Poll::Pending
        }
    }
}

impl FusedStream for Interval {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl Unpin for Interval {}


#[cfg(test)]
mod tests {
    use futures::prelude::*;
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

    #[test]
    fn intervals() {
        use futures::select;

        let mut timeout = Delay::new(Duration::from_secs(1));
        let mut stream = Interval::new(Duration::from_millis(99));

        let work = async {
            let mut total = 0;
            loop {
                select! {
                    _ = stream.next() => total += 1,
                    _ = timeout => break,
                }
            }
            

            total
        };

        let res = block_on(work);
        assert_eq!(res, 10);
    }

    #[test]
    fn send_timers() {
        const NUM_TIMERS: usize = 5;

        use futures::executor::ThreadPool;
        use futures::task::SpawnExt;
        use futures::channel::mpsc;
        let mut handle = ThreadPool::new().unwrap();

        async fn delay(value: usize, millis: u64) -> usize {
            let _ = await!(Delay::new(Duration::from_millis(millis)));

            value
        }

        let mut pool = handle.clone();
        let work = async move {
            
            let mut res: Vec<usize> = vec![];
            let (send, mut recv) = mpsc::channel(NUM_TIMERS);
            
            for i in 1..=NUM_TIMERS {
                let mut send = send.clone();
                let task = async move {
                    let v = await!(delay(i, (i*10) as u64));
                    dbg_println!("sending? {:?}", v);
                    let res = await!(send.send(v));
                    dbg_println!("result? {:?}", res);
                };
                pool.spawn(task).unwrap();
            }
            
            drop(send);
            while let Some(v) = await!(recv.next()) {
                dbg_println!("recieved {}", v);
                res.push(v);
            }
            

            res
        };

        let res = handle.run(work);
        assert_eq!(res, vec![1,2,3,4,5]);
    }
}

