use std::pin::Pin;
use std::time::{Duration, Instant};

use futures::prelude::*;
use futures::stream::FusedStream;
use futures::task::{Poll, Waker};

use super::Timer;

#[derive(Debug)]
pub struct Interval {
    inner: Timer,
    interval: Duration,
}

impl Interval {
    pub fn new(interval: Duration) -> Self {
        let inner = Timer::new();

        Interval { inner, interval }
    }
}

impl Stream for Interval {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, lw: &Waker) -> Poll<Option<Self::Item>> {
        if !self.inner.is_active() {
            let interval = self.interval;
            self.inner.handle.init_interval(interval);
        }

        self.inner.register_waker(lw);
        if self.inner.is_done() {
            self.inner.state.set_done(false);
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
