use std::hash::Hash;
use std::marker::PhantomData;

use futures::Stream;
use futures::StreamExt;
use hashbrown::HashMap;

use crate::handler::Bundle;
use crate::handler::Call;
use crate::handler::State;

pub fn resolve_globals<K, H: Clone>(handlers: &mut HashMap<K, Vec<H>>, global_handlers: &[H], key: impl Fn(&H) -> u8) {
    for list in handlers.values_mut() {
        list.extend(global_handlers.iter().cloned());
        list.sort_unstable_by_key(&key);
    }
}

pub trait MessageKey<Key> {
    fn message_key(&self) -> Key;
}

pub struct Processor<Key, Req, Res, H: Call<Req, Res> = crate::RegisteredHandler<Req, Res>> {
    handlers: HashMap<Key, Vec<H>>,
    global_handlers: Vec<H>,
    _phantom: PhantomData<(Req, Res)>,
}

impl<Key, Req, Res, H: Call<Req, Res>> Processor<Key, Req, Res, H> where Key: Eq + Hash {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            global_handlers: Vec::new(),
            _phantom: PhantomData,
        }
    }

    pub fn register(&mut self, message_key: Key, handler: H) {
        self.handlers.entry(message_key).or_default().push(handler);
    }

    pub fn register_global(&mut self, handler: H) {
        self.global_handlers.push(handler);
    }

    pub fn resolve(&mut self) where H: Clone {
        resolve_globals(&mut self.handlers, &self.global_handlers, |h| h.priority());
    }

    pub fn process<Item, InnerStream>(&self, stream: InnerStream) -> impl Stream<Item = Res> + '_
    where
        Key: Clone + Send + 'static,
        Req: Send + 'static,
        Res: Send + 'static + Default,
        H: Clone,
        Item: MessageKey<Key> + Into<Req> + Send + 'static,
        InnerStream: Stream<Item = Item> + Send + 'static,
    {
        stream.then(move |item| {
            let key = item.message_key();
            let request: Req = item.into();
            let bundle = Bundle {
                request,
                state: State::new(),
                response: Res::default(),
            };

            let handlers = self.handlers.get(&key).unwrap_or(&self.global_handlers);
            async move {
                let mut bundle = bundle;
                for handler in handlers {
                    bundle = handler.clone().call(bundle).await;
                }
                bundle.response
            }
        })
    }
}
