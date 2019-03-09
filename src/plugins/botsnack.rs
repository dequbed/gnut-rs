use futures::{future, Future};

use crate::pipes::SendMessage;
use crate::command::make_reply;

use random::{self, Source};

pub struct Snack {
    snackreplies: Vec<String>,
    random: random::Default,
}

impl Snack {
    pub fn new() -> Self {
        Snack {
            snackreplies: vec!["Thank you! >^.^<", "/me happily noms in the corner", "For me? :o", "^humansnack"].into_iter().map(|s| s.into()).collect(),
            random: random::Default::new(),
        }
    }
    pub fn call(&mut self, message: SendMessage) -> impl Future<Item = Option<SendMessage>, Error = ()> {
        if let Some(ref body) = &message.body {
            if body.0.starts_with("^goodbot") {
                return future::ok(Some(make_reply(&message, ">^.^<".into())));
            } else if body.0.starts_with("^botsnack") {
                let idx = self.random.read_u64() as usize;
                let clamped = idx % self.snackreplies.len();
                return future::ok(Some(make_reply(&message, self.snackreplies[clamped as usize].clone())));
            }
        }

        future::ok(None)
    }
}
