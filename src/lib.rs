#![feature(futures_api, async_await, await_macro)]

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

use futures::task::{AtomicWaker, Waker};

#[macro_export]
macro_rules! dbg_println {
    ($($t:tt)*) => {
        #[cfg(test)]
        {
            println!($($t)*);
        }
    };
}

mod delay;
mod interval;
mod timeout;

#[cfg(windows)]
#[path = "sys/windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "sys/linux.rs"]
mod imp;

#[cfg(target_os = "macos")]
#[path = "sys/macos.rs"]
mod imp;

use imp::NativeTimer;

pub use delay::Delay;
pub use interval::Interval;
pub use timeout::{FutureExt, Timeout, TimeoutError};

#[derive(Debug)]
pub(crate) struct TimerState {
    wake: AtomicWaker,
    done: AtomicBool,
}

impl TimerState {
    fn new() -> Self {
        TimerState {
            wake: AtomicWaker::new(),
            done: false.into(),
        }
    }

    fn register_waker(&self, lw: &Waker) {
        self.wake.register(lw);
    }

    fn set_done(&self, done: bool) {
        self.done.store(done, SeqCst);
    }

    fn done(&self) -> bool {
        self.done.load(SeqCst)
    }
}

#[derive(Debug)]
struct Timer {
    handle: NativeTimer,
    state: Arc<TimerState>,
}

impl Timer {
    pub fn new() -> Self {
        let state = Arc::new(TimerState::new());

        unsafe {
            let ptr = Arc::into_raw(state);
            let handle = NativeTimer::new(ptr as *mut _);
            let state = Arc::from_raw(ptr);

            Timer { handle, state }
        }
    }

    fn register_waker(&self, lw: &Waker) {
        self.state.register_waker(lw);
    }

    fn is_active(&self) -> bool {
        self.handle.is_active()
    }

    fn is_done(&self) -> bool {
        self.state.done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::prelude::*;
    use std::time::{Duration, Instant};

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

        use futures::channel::mpsc;
        use futures::executor::ThreadPool;
        use futures::task::SpawnExt;
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
                    let v = await!(delay(i, (i * 10) as u64));
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
        assert_eq!(res, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn send_then_drop() {
        use futures::select;
        use std::thread;

        let mut short = thread::spawn(move || Delay::new(Duration::from_millis(400)))
            .join()
            .unwrap();
        let mut long = Delay::new(Duration::from_millis(800));

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
    fn timeout() {
        use futures::future::empty;

        // The empty future will always return Poll::Pending, so this will always timeout first
        let result: Result<(), TimeoutError> = block_on(empty().timeout(Duration::new(0, 0)));
        assert!(result.is_err());
    }
}
