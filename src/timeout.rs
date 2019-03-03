use crate::Delay;
use futures::{
    prelude::*,
    task::{Poll, Waker},
};
use pin_utils::unsafe_pinned;
use std::{error, fmt, pin::Pin, time::Duration};

pub trait FutureExt {
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        let delay = Delay::new(timeout);
        Timeout {
            future: self,
            delay,
        }
    }
}

impl<F, T> FutureExt for F where F: Future<Output = T> {}

#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Timeout<F> {
    future: F,
    delay: Delay,
}

impl<F> Timeout<F> {
    unsafe_pinned!(future: F);

    unsafe_pinned!(delay: Delay);
}

impl<F: Unpin> Unpin for Timeout<F> {}

impl<F, T> Future for Timeout<F>
where
    F: Future<Output = T>,
{
    type Output = Result<T, TimeoutError>;

    fn poll(mut self: Pin<&mut Self>, w: &Waker) -> Poll<Self::Output> {
        // Check if timed out
        if let Poll::Ready(_) = self.as_mut().delay().poll(w) {
            Poll::Ready(Err(TimeoutError))
        } else {
            // Poll main future
            self.as_mut().future().poll(w).map(Ok)
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TimeoutError;
impl error::Error for TimeoutError {}
impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "future timed out")
    }
}
