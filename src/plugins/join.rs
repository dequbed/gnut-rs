use futures::{future, Future};

use xmpp_parsers::{Jid, Element};
use xmpp_parsers::presence::{Presence, Show as PresenceShow, Type as PresenceType};

use crate::pipes::SendMessage;
use crate::command::make_reply;

pub struct Join;

impl Join {
    pub fn new() -> Self {
        Join
    }

    pub fn call(&mut self, message: SendMessage) -> impl Future<Item = Option<Element>, Error = ()> {
        if let Some(ref body) = &message.body {
            if body.0.starts_with("^join") {
                let mut chan = (body.0)[6..].split_whitespace();
                if let Some(n) = chan.next() {
                    if let Some(d) = chan.next() {
                        let j = Jid::full(n,d,"gnutbot");
                        let mut presence = Presence::new(PresenceType::None);
                        presence.show = PresenceShow::None;
                        presence.to = Some(j);

                        return future::ok(Some(presence.into()));
                    }
                }
            }
        }

        future::ok(None)
    }
}
