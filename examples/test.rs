#![feature(futures_api, async_await, await_macro)]

use futures_native_timers::{Delay, Interval};
use std::time::Duration;

use futures::select;
use futures::executor::block_on;
use futures::prelude::*;

fn main() {
            

        let mut timeout = Delay::new(Duration::from_secs(1));
        let mut stream = Interval::new(Duration::from_millis(99));

        println!("{:?}", timeout);

        let work = async {
            let mut total = 0;
            loop {
                select! {
                    _ = stream.next() => {
                        total += 1;
                        println!("ping.");
                    },
                    _ = timeout => break,
                }
            }
            

            total
        };

        let res = block_on(work);
        dbg!(res);
}
