use std::task::{Context, Poll};
use std::{io, pin::Pin};

use async_channel::Sender;
use futures::prelude::*;
use md5::{Digest, Md5};

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

    /// Consumes the `ProgressReader` and returns the wrapped inner reader.
    /// Use this after the copy is complete to recover e.g. a [`HashingReader`]
    /// and call its `finish()` method.
    pub fn into_inner(self) -> R {
        self.inner
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

/// Wraps an `AsyncRead` and computes an MD5 digest of all bytes that pass
/// through it. Call [`HashingReader::finish`] after the stream reaches EOF
/// to obtain the hex-encoded digest.
pub struct HashingReader<R> {
    inner: R,
    state: Md5,
}

impl<R: AsyncRead + Unpin> HashingReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            state: Md5::new(),
        }
    }

    /// Consumes the reader and returns the hex-encoded MD5 digest of all bytes
    /// that were read through it so far.
    pub fn finish(self) -> String {
        hex::encode(self.state.finalize())
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for HashingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(n)) = &result {
            self.state.update(&buf[..*n]);
        }
        result
    }
}
