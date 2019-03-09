use futures::{Sink, Stream, Poll, Async, StartSend, IntoFuture, future, Future, AsyncSink, task};
use futures::sync::mpsc::{Sender, Receiver, channel};
use futures::future::{FutureResult, Either};
use tokio_xmpp::Event;
use xmpp_parsers::{Jid, Element, TryFrom};
use xmpp_parsers::message::{Body, Message, MessageType};
use xmpp_parsers::delay::Delay;
use tokio::runtime::current_thread::TaskExecutor;

use crate::pipes::SendMessage;

use std::usize;
use std::collections::VecDeque;

use crate::plugins::{Quotes, Snack};

pub struct CommandModule {
    futures: VecDeque<Box<Future<Item = Option<SendMessage>, Error = ()>>>,
    notify: Option<task::Task>,

    q: Quotes,
    s: Snack,
}

impl CommandModule {
    pub fn new() -> Self {
        Self {
            futures: VecDeque::new(),
            notify: None,
            q: Quotes::new(vec!["Test".into()]),
            s: Snack::new(),
        }
    }

}

impl Sink for CommandModule {
    type SinkItem = SendMessage;
    type SinkError = ();

    fn start_send(&mut self, message: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Some(_) = message.delay {
            return Ok(AsyncSink::Ready);
        }

        let mm = message.clone();
        let f = self.q.call(message.clone()).into_future().map(|o| o.map(|m| cleanup(mm, m)));
        self.futures.push_back(Box::new(f));

        let mm = message.clone();
        let g = self.s.call(message.clone()).into_future().map(|o| o.map(|m| cleanup(mm, m)));
        self.futures.push_back(Box::new(g));

        if let Some(t) = self.notify.take() {
            t.notify();
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        println!("PollCompleting");
        if self.futures.len() == 0 {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}
impl Stream for CommandModule {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.notify = Some(task::current());
        while let Some(mut f) = self.futures.pop_front() {
            println!("Polling2");
            match f.poll()? {
                Async::Ready(Some(m)) => {
                    let xm: Message = m.into();
                    return Ok(Async::Ready(Some(xm.into())));
                }
                Async::Ready(None) => {},
                Async::NotReady => self.futures.push_back(f),
            }
        }

        return Ok(Async::NotReady);
    }
}

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
