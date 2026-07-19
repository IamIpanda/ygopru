use std::hash::Hash;
use std::marker::PhantomData;

use futures::Stream;
use futures::StreamExt;
use hashbrown::HashMap;
use ygopro_data::complex;
use ygopro_data::every_client_to_server_flat_message;
use ygopro_data::every_game_message_flat_message;
use ygopro_data::every_server_to_client_flat_message;
use ygopro_data::message::ctos;
use ygopro_data::message::game_message as gm;
use ygopro_data::message::stoc;

use crate::handler::Bundle;
use crate::handler::Call;

pub fn resolve_globals<K, H: Clone>(handlers: &mut HashMap<K, Vec<H>>, global_handlers: &[H], key: impl Fn(&H) -> u8) {
    for list in handlers.values_mut() {
        list.extend(global_handlers.iter().cloned());
        list.sort_unstable_by_key(&key);
    }
}

pub trait MessageKey<Key> {
    fn message_key(&self) -> Key;
}

impl MessageKey<u8> for u8 {
    fn message_key(&self) -> u8 {
        *self
    }
}

impl<Message> MessageKey<u8> for complex::Complex<Message> {
    fn message_key(&self) -> u8 {
        self.data[0]
    }
}

macro_rules! impl_message_key {
    ($message_mod:ident, $($variant:ident = $flag:literal),* $(,)?) => {
        impl $crate::processor::MessageKey<u8> for $message_mod::MessageType {
            fn message_key(&self) -> u8 {
                u8::from(self)
            }
        }
        impl $crate::processor::MessageKey<u8> for $message_mod::Message {
            fn message_key(&self) -> u8 {
                match self {
                    $($message_mod::Message::$variant(_) => $flag),*
                }
            }
        }
        $(
            impl $crate::processor::MessageKey<u8> for $message_mod::$variant {
                fn message_key(&self) -> u8 {
                    $flag
                }
            }
        )*
    };
}

macro_rules! impl_ctos_message_key { ($($rest:tt)*) => { impl_message_key!(ctos, $($rest)*); }; }
macro_rules! impl_stoc_message_key { ($($rest:tt)*) => { impl_message_key!(stoc, $($rest)*); }; }
macro_rules! impl_gm_message_key { ($($rest:tt)*) => { impl_message_key!(gm, $($rest)*); }; }

every_client_to_server_flat_message!(impl_ctos_message_key);
every_server_to_client_flat_message!(impl_stoc_message_key);
every_game_message_flat_message!(impl_gm_message_key);

pub struct Processor<Key, Req, State = crate::handler::State, Res = (), H: Call<Req, State, Res> = crate::handler::tower_handler::TowerHandler<Req, State, Res>> {
    handlers: HashMap<Key, Vec<H>>,
    global_handlers: Vec<H>,
    _phantom: PhantomData<(Req, State, Res)>,
}

impl<Key, Req, State, Res, H: Call<Req, State, Res>> Processor<Key, Req, State, Res, H>
where
    Key: Eq + Hash,
    State: Send + Sync,
{
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

    pub fn process<'a, Item, InnerStream, AssembleBundle>(&'a self, stream: InnerStream, assemble_bundle: AssembleBundle) -> impl Stream<Item = Res> + 'a
    where
        Key: Clone + Send + 'static,
        Req: Send + 'static,
        Res: Send + 'static,
        Item: MessageKey<Key> + Into<Req> + Send + 'static,
        InnerStream: Stream<Item = Item> + Send + 'static,
        AssembleBundle: Fn(Req) -> Bundle<Req, State, Res> + 'a,
    {
        stream.then(move |item| {
            let key = item.message_key();
            let request: Req = item.into();
            let bundle = assemble_bundle(request);

            let handlers = self.handlers.get(&key).unwrap_or(&self.global_handlers);
            async move {
                let mut bundle = bundle;
                for handler in handlers {
                    bundle = handler.call(bundle).await;
                }
                bundle.response
            }
        })
    }
}

/// Bundle factory for [`Default`]-constructible types.
///
/// Pass this as the `assemble_bundle` argument to [`Processor::process`].
pub fn default_bundle<Req, State: Default, Res: Default>(request: Req) -> Bundle<Req, State, Res> {
    Bundle {
        request,
        state: State::default(),
        response: Res::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::MessageKey;
    use ygopro_data::message::ctos;

    #[test]
    fn join_game_is_message_key() {
        let join = ctos::JoinGame {
            version: 0x1338,
            gameid: 0,
            pass: ygopro_data::string::FixedLengthString::from(""),
        };
        let key: u8 = join.message_key();
        assert_eq!(key, 18);
    }
}
