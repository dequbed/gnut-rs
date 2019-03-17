use futures::{Sink, Stream, Poll, Async, StartSend, Future, AsyncSink, task};
use xmpp_parsers::Element;
use xmpp_parsers::message::{Body, MessageType};

use crate::sendable::SendMessage;

fn cleanup(original: SendMessage, mut m: SendMessage) -> SendMessage {
    if original.mtype == MessageType::Groupchat {
        m.mtype = MessageType::Groupchat;
        m.to = original.from.map(|j| j.into_bare_jid());
    } else {
        m.to = original.from;
        m.from = None;
    }

    m
}

pub fn make_reply(msg: &SendMessage, body: String) -> SendMessage {
    SendMessage {
        from: None,
        to: None,
        id: msg.id.clone(),
        mtype: msg.mtype.clone(),
        body: Some(Body(body)),
        subject: msg.subject.clone(),
        thread: msg.thread.clone(),
        delay: None,
    }
}

pub trait SyncMessagePlugin {
    fn call(&mut self, e: SendMessage) -> Option<SendMessage>;
}
pub trait SyncPlugin {
    fn call(&mut self, e: SendMessage) -> Option<Element>;
}
pub trait FutureMessagePlugin {
    fn call(&mut self, e: SendMessage) -> Box<Future<Item=Option<SendMessage>, Error=()>>;
}
pub trait FuturePlugin {
    fn call(&mut self, e: SendMessage) -> Box<Future<Item=Option<Element>, Error=()>>;
}
pub trait StreamPlugin {
    fn call(&mut self, e: SendMessage) -> Box<Stream<Item=Element, Error=()>>;
}


impl<P: SyncMessagePlugin> SyncPlugin for P {
    fn call(&mut self, e: SendMessage) -> Option<Element> {
        SyncMessagePlugin::call(self, e.clone())
            .map(move |m| cleanup(e, m))
            .map(|m| m.into())
    }
}
impl<P: FutureMessagePlugin> FuturePlugin for P {
    fn call(&mut self, e: SendMessage) -> Box<Future<Item=Option<Element>, Error=()>> {
        Box::new(FutureMessagePlugin::call(self, e.clone())
            .map(|o| o
            .map(move |m| cleanup(e, m))
            .map(|m| m.into())))
    }
}
impl<P: FuturePlugin> StreamPlugin for P {
    fn call(&mut self, e: SendMessage) -> Box<Stream<Item=Element, Error=()>> {
        use crate::into_stream;
        Box::new(into_stream::new(FuturePlugin::call(self, e)))
    }
}


use futures::task::Task;
pub struct Combinator {
    blocked_send: Option<Task>,
    blocked_recv: Option<Task>,

    value: Option<SendMessage>,
    inner: Box<StreamPlugin>,
}
impl Combinator {
    pub fn new(inner: Box<StreamPlugin>) -> Self {
        Self {
            blocked_send: None,
            blocked_recv: None,
            value: None,
            inner,
        }
    }
}
impl Sink for Combinator {
    type SinkItem = SendMessage;
    type SinkError = ();

    fn start_send(&mut self, m: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match self.value {
            None => {
                self.value = Some(m);
                if let Some(t) = self.blocked_recv.take() {
                    t.notify();
                }

                Ok(AsyncSink::Ready)
            },
            Some(_) => {
                self.blocked_send = Some(task::current());
                if let Some(t) = self.blocked_recv.take() {
                    t.notify();
                }

                Ok(AsyncSink::NotReady(m))
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        if self.value.is_none() && self.blocked_send.is_none() {
            return Ok(Async::Ready(()));
        }
        if let Some(t) = self.blocked_recv.take() {
            t.notify();
        }
        return Ok(Async::NotReady);
    }
}
impl Stream for Combinator {
    type Item = Box<Stream<Item=Element, Error=()>>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.value.take() {
            Some(m) => {
                if let Some(t) = self.blocked_send.take() {
                    t.notify();
                }
                Ok(Async::Ready(Some(self.inner.call(m))))
            },
            None => {
                self.blocked_recv = Some(task::current());
                Ok(Async::NotReady)
            },
        }
    }
}

use futures::stream::{Flatten, SplitStream, SplitSink};
pub struct CombinatorModule {
    sinks: Vec<SplitSink<Combinator>>,
    streams: Vec<Flatten<SplitStream<Combinator>>>,
}
impl CombinatorModule {
    pub fn new_with_modules(mods: Vec<Combinator>) -> Self {
        use std::iter::Iterator;
        let (sinks, streams): (_, Vec<SplitStream<Combinator>>) = mods.into_iter().map(|m| m.split()).unzip();
        let streams = streams.into_iter().map(|s| s.flatten()).collect();
        Self { 
            sinks,
            streams,
        }
    }

    pub fn new() -> Self {
        Self::new_with_modules(Vec::new())
    }
}
impl Sink for CombinatorModule {
    type SinkItem = SendMessage;
    type SinkError = ();

    fn start_send(&mut self, m: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        println!("C {:?}", m);
        // We have absolutely no reason to keep delay messages at the moment
        // FIXME: Figure out a way to handle delayed messages sensibly
        if m.delay.is_some() {
            return Ok(AsyncSink::Ready);
        }

        let mut ready = true;
        for s in self.sinks.iter_mut() {
            ready = ready | match s.poll_complete()? {
                Async::Ready(()) => true,
                Async::NotReady => false
            }
        }
        if ready {
            for s in self.sinks.iter_mut() {
                s.start_send(m.clone()).expect("Combinator signaled NotReady after flushing completely!");
            }

            Ok(AsyncSink::Ready)
        } else {
            Ok(AsyncSink::NotReady(m))
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        for s in self.sinks.iter_mut() {
            try_ready!(s.poll_complete());
        }
        Ok(Async::Ready(()))
    }
}
impl Stream for CombinatorModule {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        for s in self.streams.iter_mut() {
            if let Async::Ready(e) = s.poll()? {
                return Ok(Async::Ready(e));
            }
        }

        Ok(Async::NotReady)
    }
}
