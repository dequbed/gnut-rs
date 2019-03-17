use futures::{future, Future};

use random::{self, Source};

use crate::sendable::SendMessage;
use crate::command::make_reply;
use crate::command::FutureMessagePlugin;

pub struct Quotes {
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
}
impl FutureMessagePlugin for Quotes {
    fn call(&mut self, message: SendMessage) -> Box<Future<Item = Option<SendMessage>, Error = ()>> {
        if let Some(ref body) = &message.body {
            if body.0.starts_with("^quote") {
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
                    if self.quotes.len() == 0 {
                        return Box::new(future::ok(None));
                    }

                    // Random
                    let idx = self.random.read_u64() as usize;
                    let clamped = idx % self.quotes.len();
                    out = self.quotes[clamped as usize].clone();
                }
                return Box::new(future::ok(Some(make_reply(&message, out))))
            }
        }

        Box::new(future::ok(None))
    }
}
