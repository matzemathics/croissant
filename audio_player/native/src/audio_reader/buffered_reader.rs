
use std::{
    iter::Iterator,
    result::Result,
    pin::Pin,
    task::{Context, Poll},
    sync::{Arc, Mutex}
};

use futures::{ sink::Sink, task::AtomicWaker };

pub trait ReaderTarget<T> : core::marker::Sync + core::marker::Send {
    fn read_iter <I: Iterator<Item = T>> (&mut self, iter: &mut I) -> usize;
    fn read_value (&mut self, val: T) -> Result<(), T>;
    fn is_full(&self) -> bool;

    fn read(&mut self, buffer: &mut Vec<T>) {
        let mut iter = buffer.drain(0..);
        self.read_iter(&mut iter);
        let mut rest : Vec<T> = iter.collect();
        buffer.append(&mut rest);
    }
}

pub struct BufferedReader<Item, R> {
    buffer: Option<Vec<Item>>,
    target: Arc<Mutex<R>>,
    waker: Arc<AtomicWaker>
}

impl<Item, R> BufferedReader<Item, R>
where
    R: ReaderTarget<Item>
{
    pub fn new (target: Arc<Mutex<R>>, waker: Arc<AtomicWaker>) -> BufferedReader<Item, R>
    { 
        BufferedReader {
            buffer: None,
            target: target,
            waker: waker
        }
    }
}

impl<Item, R> Sink<Vec<Item>> for BufferedReader<Item, R>
where
    Item: Unpin,
    R: ReaderTarget<Item>
{
    type Error = ();


    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context
    ) -> Poll<Result<(), Self::Error>>
    { 
        Poll::Ready(Ok(())) 
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<Item>) -> Result<(), Self::Error>
    {
        let this = Pin::into_inner(self);

        let mut target = this.target.lock().unwrap();

        if target.is_full() {
            this.buffer = Some(item);
            return Ok(())
        }

        let mut buf = item;


        target.read(&mut buf);

        if buf.is_empty() {
            Ok(())
        } 
        else {
            this.buffer = Some(buf);
            Ok(())
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context
    ) -> Poll<Result<(), Self::Error>>
    {
        if self.buffer.is_none() {
            return Poll::Ready(Ok(()))
        }

        self.waker.register(cx.waker());

        let this = Pin::into_inner(self);

        let mut target = this.target.lock().unwrap();

        if target.is_full() {
            return Poll::Pending;
        }

        let buf = this.buffer.as_mut().unwrap();
        
        target.read(buf);


        if buf.is_empty() {
            this.buffer = None;
            Poll::Ready(Ok(()))
        } 
        else {
            Poll::Pending
        }
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context
    ) -> Poll<Result<(), Self::Error>>
    {
        self.poll_flush(cx)
    }
}
