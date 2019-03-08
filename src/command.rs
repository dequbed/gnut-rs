use futures::{Sink, Stream, Poll, Async, StartSend, IntoFuture, future, Future, AsyncSink};
use futures::sync::mpsc::{Sender, Receiver, channel};
use futures::future::{FutureResult, Either};
use tokio_xmpp::Event;
use xmpp_parsers::{Jid, Element, TryFrom};
use xmpp_parsers::message::{Body, Message, MessageType};
use xmpp_parsers::delay::Delay;
use tokio::runtime::current_thread::TaskExecutor;

use random::{self, Source};
use std::usize;

pub struct CommandModule {
    e: TaskExecutor,
    tx: Sender<(Jid, String)>,
    rx: Receiver<(Jid, String)>,

    q: Quotes,
}

impl CommandModule {
    pub fn new(e: TaskExecutor) -> Self {
        let (tx, rx) = channel(5);
        Self {
            e,
            tx,
            rx,
            q: Quotes::new(vec!["Test".into()])
        }
    }
}

impl Sink for CommandModule {
    type SinkItem = Message;
    type SinkError = ();

    fn start_send(&mut self, message: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        for p in message.payloads.iter() {
            if let Ok(_) = Delay::try_from(p.clone()) {
                return Ok(AsyncSink::Ready);
            }
        }

        println!("Received {:?}", message);

        if self.q.filter(&message) {
            println!("calling");
            let f = self.q.call(message, self.tx.clone()).into_future();
            self.e.spawn_local(Box::new(f)).unwrap();
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}
impl Stream for CommandModule {
    type Item = Element;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some((j,m)) = try_ready!(self.rx.poll()) {
            let mut type_ = MessageType::Chat;
            if is_groupchat(&j) {
                type_ = MessageType::Groupchat;
            }
            let mut message = Message::new(Some(j));
            message.type_ = type_;
            if message.type_ == MessageType::Groupchat {
                message.to = Some(Jid::into_bare_jid(message.to.unwrap()));
            }
            message.bodies.insert(String::new(), Body(m));
            Ok(Async::Ready(Some(message.into())))
        } else {
            Ok(Async::NotReady)
        }
    }
}

fn is_groupchat(j: &Jid) -> bool {
    j.domain == "chat.paranoidlabs.org" && j.node.is_some()
}

struct Quotes {
    quotes: Vec<String>,
    random: random::Default,
}
impl Quotes {
    pub fn new(q: Vec<String>) -> Self{
        Self {
            quotes: q,
            random: random::Default::new(),
        }
    }

    fn filter(&mut self, message: &Message) -> bool {
        if let (Some(ref _from), Some(ref body)) = (&message.from, message.bodies.get("")) {
            if body.0.starts_with("^quote") {
                return true;
            }
        }

        return false;
    }

    fn call(&mut self, message: Message, tx: Sender<(Jid, String)>) -> impl Future<Item = (), Error = ()> {
        match (message.from, message.bodies.get("")) {
            (Some(ref from), Some(ref body)) => {
                let mut args = body.0.split_whitespace();
                let out;
                if let Some(a) = args.nth(1) { 
                    if a == "add" {
                        let mut q = String::new();
                        for a in args {
                            q.push_str(a);
                            q.push_str(" ");
                        }

                        if q.trim().is_empty() {
                            out = format!("Invalid quote");
                        } else {
                            self.quotes.push(q);
                            out = format!("Added new quote #{}", self.quotes.len()-1);
                        }
                    } else if let Ok(idx) = usize::from_str_radix(a, 10) {
                        if idx >= self.quotes.len() {
                            out = format!("Invalid quote #{}", idx)
                        } else {
                            out = self.quotes[idx].clone();
                        }
                    } else {
                        out = format!("Usage: ^quote <index>|add quote")
                    }
                } else {
                    // Random
                    let idx = self.random.read_u64() as usize;
                    let clamped = idx % self.quotes.len();
                    out = self.quotes[clamped as usize].clone();
                }
                return Either::A(tx.send((from.clone(), out)).map(|_| {}).map_err(|_| {}))
            },
            _ => return Either::B(future::ok(()))
        }
    }
}
