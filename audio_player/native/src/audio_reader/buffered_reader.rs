
//+--------------------------------------------------------------------+
//| buffered_reader.rs - definiert einen Datentyp (BufferedReader<T>)  |
//|      für asynchrone Schreibvorgänge. Der Empfänger der Daten muss  |
//|      dafür lediglich das tarit ReaderTarget<T> implementieren.     |
//+--------------------------------------------------------------------+

use std::{
    iter::Iterator,
    result::Result,
    pin::Pin,
    task::{Context, Poll},
    sync::{Arc, Mutex}
};

use futures::{ sink::Sink, task::AtomicWaker };

//+-------------------------------------------------------
//| trait ReaderTarget<T>
//|     - Objekte die beliebig viele Daten vom Typ T 
//|       aufnehmen können. Mit der Funktion is_full()
//|       wird signalisiert, dass momentan keine weiteren
//|       Daten aufgenommen werden können, also gewartet
//|       werden muss, bis erneut Daten gesendet werden
//|       können.

pub trait ReaderTarget<T> : core::marker::Sync + core::marker::Send {
    fn read_iter <I: Iterator<Item = T>> (&mut self, iter: &mut I) -> usize;
    fn is_full(&self) -> bool;

    fn read(&mut self, buffer: &mut Vec<T>) {
        let mut iter = buffer.drain(0..);
        self.read_iter(&mut iter);
        let mut rest : Vec<T> = iter.collect();
        buffer.append(&mut rest);
    }
}

//+-------------------------------------------------------
//| struct BufferedReader<Item, R>
//|     - sendet Daten vom Typ Item an ein Objekt vom
//|       Typ R, wobei R das trait ReaderTarget<Item>
//|       implementieren muss.

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

    pub fn ready (&self) -> bool {
        ! self.target.lock().unwrap().is_full()
    }
}

// Für Buffered Reader wird das trait Sink implementiert,
// das asynchrones Versenden von Daten ermöglicht. In
// diesem Fall werden Daten in R geschrieben, bis R keine
// weiteren Daten mehr aufnehmen kann. Diese werden dann 
// in BufferedReader::buffer gepuffert. Sobald der Waker
// (BufferedReader::waker) signalisiert, dass wieder Daten
// gesendet werden können, werden die gepufferten Daten
// übermittelt.

impl<Item, R> Sink<Vec<Item>> for BufferedReader<Item, R>
where
    Item: Unpin,
    R: ReaderTarget<Item>
{
    type Error = ();

    // Gibt an, ob das Sink bereit zum Senden ist.
    // Da hier keine Vorbereitungen getroffen werden müssen,
    // wird immer Ok zurückgegeben.
    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context
    ) -> Poll<Result<(), Self::Error>>
    { 
        Poll::Ready(Ok(())) 
    }

    // Beginnt einen asychronen Sendevorgang, wobei versucht wird,
    // soviele Daten wie möglich direkt an R zu senden.
    fn start_send(self: Pin<&mut Self>, item: Vec<Item>) -> Result<(), Self::Error>
    {
        let this = Pin::into_inner(self);
        let mut target = this.target.lock().unwrap();

        // wenn der Empfänger keine Daten aufnehmen kann
        if target.is_full() {
            // werden diese vollständig gepuffert
            this.buffer = Some(item);
            return Ok(())
        }

        let mut buf = item;
        target.read(&mut buf);

        if buf.is_empty() {
            // Puffer vollständig versendet,
            // Vorgang bereits abgeschlossen
            Ok(())
        } 
        else {
            // Puffer teilweise versendet,
            // übrige Daten in buffer speichern
            this.buffer = Some(buf);
            Ok(())
        }
    }

    // asynchroner Vorgang, der Daten vollständig versendet
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context
    ) -> Poll<Result<(), Self::Error>>
    {
        // wenn der Puffer bereits gelehrt ist
        if self.buffer.is_none() {
            // Ende des Vorgangs
            return Poll::Ready(Ok(()))
        }

        // Vorgang wird fortgesetzt, wenn der
        // Waker das Signal gibt
        self.waker.register(cx.waker());

        let this = Pin::into_inner(self);
        let mut target = this.target.lock().unwrap();

        // wenn keine Daten mehr gesendet werden können
        if target.is_full() {
            // Warten, bis wieder Daten gesendet werden können
            return Poll::Pending;
        }

        let buf = this.buffer.as_mut().unwrap();

        // Daten so weit wie möglich einlesen
        target.read(buf);

        // Wenn der Puffer gelehrt wurde
        if buf.is_empty() {
            // Ende des Vorgangs
            this.buffer = None;
            Poll::Ready(Ok(()))
        } 
        else {
            // Warten, bis wieder Daten gesendet werden können
            Poll::Pending
        }
    }

    // sendet Daten und schließt die Verbindung
    // Da hier keine besonderen Aktionen notwendig sind,
    // einfach nur synonym für poll_flush
    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context
    ) -> Poll<Result<(), Self::Error>>
    {
        self.poll_flush(cx)
    }
}
