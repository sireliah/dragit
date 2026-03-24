use std::task::{Context, Poll};
use std::{io, pin::Pin};

use async_channel::Sender;
use futures::prelude::*;

use crate::p2p::peer::{Direction, PeerEvent};
use crate::p2p::util;

pub struct ProgressReader<R> {
    inner: R,
    size: usize,
    counter: usize,
    current_size: usize,
    sender_queue: Sender<PeerEvent>,
    direction: Direction,
}

impl<R: AsyncRead + Unpin> ProgressReader<R> {
    pub fn new(
        inner: R,
        size: usize,
        sender_queue: Sender<PeerEvent>,
        direction: Direction,
    ) -> Self {
        Self {
            inner,
            size,
            counter: 0,
            current_size: 0,
            sender_queue,
            direction,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(n)) = result {
            self.counter += n;
            self.current_size += n;
            if util::time_to_notify(self.current_size, self.size) {
                let sender = self.sender_queue.clone();
                let counter = self.counter;
                let size = self.size;
                let direction = self.direction.clone();
                tokio::spawn(async move {
                    util::notify_progress(&sender, counter, size, &direction).await;
                });
                self.current_size = 0;
            }
        }
        result
    }
}
