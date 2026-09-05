//! The message dispatcher: routes messages to handlers by key.
//!
//! A [`Processor`] holds a set of handlers keyed by message type (a [`MessageKey`]),
//! plus a set of global handlers that run for every message. It dispatches each message
//! through the matching handler chain and combines their responses.

use std::hash::Hash;
use std::marker::PhantomData;

use futures::Stream;
use futures::StreamExt;
use hashbrown::HashMap;
use hashbrown::HashSet;
use ygopro_data::complex;
use ygopro_data::every_client_to_server_flat_message;
use ygopro_data::every_game_message_flat_message;
use ygopro_data::every_server_to_client_flat_message;
use ygopro_data::message::ctos;
use ygopro_data::message::game_message as gm;
use ygopro_data::message::stoc;

use crate::handler::Bundle;
use crate::handler::Call;
use crate::handler::sync_handler::SyncHandler;
use crate::handler::sync_handler::WithSubState;
use crate::extract::Request;

/// Clone the global handlers into every key list and sort each list by priority.
///
/// After resolving, the handler set should be considered frozen; mutating it afterwards
/// would break the ordering or the global-handler copies.
pub fn resolve_globals<K, H: Clone>(handlers: &mut HashMap<K, Vec<H>>, global_handlers: &[H], key: impl Fn(&H) -> u8) {
    for list in handlers.values_mut() {
        list.extend(global_handlers.iter().cloned());
        list.sort_unstable_by_key(&key);
    }
}

/// A pure flag whose only meaning is its name.
/// Used as a handler key it resolves to 0, letting the call site register
/// the handler as a global one via `register_global`.
#[derive(Debug)]
pub struct All {
    _private: (),
}

impl ygopro_data::message::PureMessage for All {}

impl ygopro_data::message::Message for All {
    fn message_type() -> ygopro_data::message::all::MessageType {
        ygopro_data::message::all::MessageType::Other("all", 0)
    }
}

/// The key that [`Processor`] used to look up the handlers for a message.
pub trait MessageKey<Key> {
    /// Get the message key.
    fn message_key(&self) -> Key;
}

impl MessageKey<u8> for u8 {
    fn message_key(&self) -> u8 {
        *self
    }
}

impl MessageKey<u8> for complex::Complex<ctos::Message> {
    fn message_key(&self) -> u8 {
        self.data[0]
    }
}

impl MessageKey<u8> for complex::Complex<stoc::Message> {
    fn message_key(&self) -> u8 {
        self.data[0]
    }
}

impl MessageKey<u8> for complex::Complex<gm::Message> {
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

impl<Message, Extra> MessageKey<u8> for Request<Message, Extra>
where
    Message: MessageKey<u8>,
{
    fn message_key(&self) -> u8 {
        self.message.message_key()
    }
}

/// The message dispatcher.
///
/// It holds handlers keyed by [`MessageKey`] and a set of global handlers, and routes
/// each message through the matching chain.
pub struct Processor<Key, Req, State = crate::handler::State, Res = (), H: Call<Req, State, Res> = crate::handler::tower_handler::TowerHandler<Req, State, Res>> {
    handlers: HashMap<Key, Vec<H>>,
    global_handlers: Vec<H>,
    _phantom: PhantomData<fn(Req, State, Res)>,
}

impl<Key, Req, State, Res, H: Call<Req, State, Res>> Processor<Key, Req, State, Res, H>
where
    Key: Eq + Hash,
    State: Send,
{
    /// Create an empty processor.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            global_handlers: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// The number of registered handlers, including globals.
    pub fn handler_count(&self) -> usize {
        self.handlers.values().map(|handlers| handlers.len()).sum::<usize>() + self.global_handlers.len()
    }

    /// Count the handlers grouped by module, dropping the duplicate global copies.
    pub fn handler_statistics(&self, module_name_of: fn(&H) -> &'static str) -> hashbrown::HashMap<&'static str, usize> {
        let mut handler_counts = hashbrown::HashMap::new();
        for handlers in self.handlers.values() {
            for handler in handlers {
                *handler_counts.entry(module_name_of(handler)).or_insert(0) += 1;
            }
        }
        let mut global_counts = hashbrown::HashMap::new();
        for handler in &self.global_handlers {
            *global_counts.entry(module_name_of(handler)).or_insert(0) += 1;
        }
        // resolve() clones each global handler into every key list, so a module
        // counts its own globals once per key list; drop the duplicate copies.
        let key_list_count = self.handlers.len();
        for (module_name, global_count) in global_counts {
            let duplicated_copies = global_count * key_list_count.saturating_sub(1);
            if let Some(count) = handler_counts.get_mut(&module_name) {
                *count -= duplicated_copies;
            }
        }
        handler_counts
    }

    /// Build a processor from builder functions, filtering by group and splitting globals.
    pub fn new_with_groups(builders: &[fn() -> (Key, H)], groups: &HashSet<String>, group_of: fn(&H) -> &'static str, is_all: impl Fn(&Key) -> bool) -> Self where H: Clone {
        let mut processor = Self::new();
        for build in builders {
            let (key, handler) = build();
            if !groups.is_empty() && !groups.contains(group_of(&handler)) {
                continue;
            }
            if is_all(&key) {
                processor.register_global(handler);
            } else {
                processor.register(key, handler);
            }
        }
        processor.resolve();
        processor
    }

    /// Register a handler for a message key.
    pub fn register(&mut self, message_key: Key, handler: H) {
        self.handlers.entry(message_key).or_default().push(handler);
    }

    /// Register a global handler that runs for every message.
    pub fn register_global(&mut self, handler: H) {
        self.global_handlers.push(handler);
    }

    /// Clone the global handlers into every key list and sort each list by priority.
    ///
    /// After resolving, the processor should not be modified anymore.
    pub fn resolve(&mut self) where H: Clone {
        resolve_globals(&mut self.handlers, &self.global_handlers, |h| h.priority());
    }

    /// Run a bundle through the handler chain for the given key, stopping on [`StopFlag`].
    pub async fn process_bundle(&self, bundle: Bundle<Req, State, Res>, key: Key) -> Bundle<Req, State, Res>
    where
        Key: Eq + Hash,
    {
        let handlers = self.handlers.get(&key).unwrap_or(&self.global_handlers);
        let mut bundle = bundle;
        for handler in handlers {
            bundle = handler.call(bundle).await;
            if bundle.stop_flag.0 { break }
        }
        bundle
    }

    /// Process a stream of messages, assembling a bundle per item and consuming the result.
    pub fn process<Item, InnerStream, AssembleBundle, ConsumeBundle>(
        self: std::sync::Arc<Self>,
        stream: InnerStream,
        assemble_bundle: AssembleBundle,
        consume_bundle: ConsumeBundle,
    ) -> impl Stream<Item = Bundle<Req, State, Res>>
    where
        Key: Clone + Eq + Hash + Send + 'static,
        Req: Send + 'static,
        Res: Send + 'static,
        State: Send + 'static,
        Item: MessageKey<Key> + Into<Req> + Send + 'static,
        InnerStream: Stream<Item = Item> + Send + 'static,
        AssembleBundle: Fn(Req) -> Bundle<Req, State, Res> + Send + 'static,
        ConsumeBundle: Fn(Bundle<Req, State, Res>) -> Bundle<Req, State, Res> + Clone + Send + 'static,
    {
        stream.then(move |item| {
            let key = item.message_key();
            let request: Req = item.into();
            let bundle = assemble_bundle(request);
            let processor = self.clone();
            let consume_bundle = consume_bundle.clone();
            async move {
                let bundle = processor.process_bundle(bundle, key).await;
                let bundle = consume_bundle(bundle);
                bundle
            }
        })
    }
}

impl<Key, Req, Target, Res> Processor<Key, Req, Target, Res, SyncHandler<Req, Target, Res>>
where
    Key: Eq + Hash,
    Req: Send + 'static,
    Target: Send + 'static,
    Res: Send + 'static + std::ops::Mul<Output = Res>,
{
    /// Build a processor from two handler sets (target and source states), merging them.
    ///
    /// The source handlers are written against `SubState` but run against `Target`. This is
    /// only sound for [`SyncHandler`], whose `Call` impl reinterprets
    /// `&mut Bundle<Req, Target, Res>` as `&mut Bundle<Req, SubState, Res>` via the
    /// [`handler::sync_handler::WithSubState`] layout guarantee.
    ///
    /// [`TowerHandler`] and [`AsyncHandler`] erase the handler behind a trait object
    /// (`BoxCloneService` / `Arc<dyn Call>`), so they cannot be transmuted between state
    /// types; the dual-state trick is unique to `SyncHandler`, which stores raw pointers
    /// and monomorphized function pointers. The layout assumption is checked by
    /// [`handler::sync_handler::assert_sync_handler_layout`] in [`extend`](Self::extend).
    pub fn new_with_dual_group<SubState>(
        target_builders: &[fn() -> (Key, SyncHandler<Req, Target, Res>)],
        source_builders: &[fn() -> (Key, SyncHandler<Req, SubState, Res>)],
        groups: &HashSet<String>,
        target_group_of: fn(&SyncHandler<Req, Target, Res>) -> &'static str,
        source_group_of: fn(&SyncHandler<Req, SubState, Res>) -> &'static str,
        is_all: impl Fn(&Key) -> bool,
    ) -> Self
    where
        SubState: Send + 'static,
        Target: WithSubState<SubState>,
    {
        let mut processor = Self::new();
        for build in target_builders {
            let (key, handler) = build();
            if !groups.is_empty() && !groups.contains(target_group_of(&handler)) { continue; }
            if is_all(&key) { processor.register_global(handler); } else { processor.register(key, handler); };
        }
        let mut source_processor = Processor::<Key, Req, SubState, Res, SyncHandler<Req, SubState, Res>>::new();
        for build in source_builders {
            let (key, handler) = build();
            if !groups.is_empty() && !groups.contains(source_group_of(&handler)) { continue; }
            if is_all(&key) { source_processor.register_global(handler); } else { source_processor.register(key, handler); };
        }
        processor.extend(source_processor);
        processor.resolve();
        processor
    }

    /// Merge the handlers of a source-state processor into this target-state processor.
    ///
    /// Each source handler is transmuted to run against `Target`. This assumes the
    /// [`handler::sync_handler::WithSubState`] layout guarantee and is checked by
    /// [`handler::sync_handler::assert_sync_handler_layout`].
    pub fn extend<SubState>(&mut self, processor_source: Processor<Key, Req, SubState, Res, SyncHandler<Req, SubState, Res>>)
    where
        SubState: Send + 'static,
        Target: WithSubState<SubState>,
    {
        crate::handler::sync_handler::assert_sync_handler_layout::<Req, SubState, Target, Res>();
        for (key, handlers) in processor_source.handlers {
            self.handlers.entry(key).or_default().extend(handlers.into_iter()
                .map(|handler| unsafe { std::mem::transmute::<SyncHandler<Req, SubState, Res>, SyncHandler<Req, Target, Res>>(handler) }));
        }
        self.global_handlers.extend(processor_source.global_handlers.into_iter()
            .map(|handler| unsafe { std::mem::transmute::<SyncHandler<Req, SubState, Res>, SyncHandler<Req, Target, Res>>(handler) }));
    }
}

/// Create a bundle from a request, with default state and response.
pub fn default_bundle<Req, State: Default, Res: Default>(request: Req) -> Bundle<Req, State, Res> {
    Bundle {
        request,
        state: State::default(),
        response: Res::default(),
        stop_flag: Default::default()
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
