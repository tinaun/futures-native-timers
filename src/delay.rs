use std::time::Duration;
use std::pin::Pin;

use futures::prelude::*;
use futures::future::FusedFuture;
use futures::task::{Poll, LocalWaker};

use super::Timer;

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