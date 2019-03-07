use futures::{Sink, Stream, Poll, Async, StartSend, IntoFuture, future, Future, AsyncSink};
use futures::sync::mpsc::{Sender, Receiver, channel};
use futures::future::{FutureResult, Either};
use tokio::runtime::TaskExecutor;
use tokio_xmpp::Event;
use xmpp_parsers::{Jid, Element, TryFrom};
use xmpp_parsers::message::{Body, Message};
use xmpp_parsers::delay::Delay;

use random::{self, Source};

struct CommandModule {
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
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Some(message) = item.into_stanza().and_then(|stanza| Message::try_from(stanza).ok()) {
            for p in message.payloads.iter() {
                if let Ok(_) = Delay::try_from(p.clone()) {
                    return Ok(AsyncSink::Ready);
                }
            }

            if self.q.filter(&message) {
                let f = self.q.call(message, self.tx.clone()).into_future();
                self.e.spawn(f);
            }
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
            let mut message = Message::new(Some(j));
            message.bodies.insert(String::new(), Body(m));
            Ok(Async::Ready(Some(message.into())))
        } else {
            Ok(Async::NotReady)
        }
    }
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
        if let (Some(ref from), Some(ref body)) = (&message.from, message.bodies.get("")) {
            if body.0.starts_with("^quote") {
                return true;
            }
        }

        return false;
    }

    fn call(&mut self, message: Message, tx: Sender<(Jid, String)>) -> impl Future<Item = (), Error = ()> {
        match (message.from, message.bodies.get("")) {
            (Some(ref from), Some(ref body)) => {
                let args: Vec<&str> = body.0.split_whitespace().collect();
                let out;
                if args.len() == 1 {
                    // Random
                    let idx = self.random.read_u64() as usize;
                    let clamped = idx % self.quotes.len();
                    out = self.quotes[clamped as usize].clone();
                } else {
                    out = "Not yet implemented".into();
                }

                return Either::A(tx.send((from.clone(), out)).map(|_| {}).map_err(|_| {}))
            },
            _ => return Either::B(future::ok(()))
        }
    }
}
