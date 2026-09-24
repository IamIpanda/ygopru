//! Wrapper of Duel instances. 
//! 
//! [`DuelHost`] is a wrapper of [`Duel`](crate::duel::Duel) instances.

use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;

use futures::Stream;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use ygopro_data::complex::Complex;
use ygopro_data::constants::Mode;
use ygopro_data::constants::Netplayer;
use ygopro_data::message::HostInfo;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::string::FixedLengthString;
use ygopro_handler::RoomProvider;

use crate::configuration::Configuration;
use crate::duel::Request;
use crate::duel::SendTarget;
use crate::single_duel::SingleDuel;
use crate::tag_duel::TagDuel;

/// A wrapper of [`SingleDuel`] or [`TagDuel`].
/// 
/// DuelHost keeps a mpsc sender from Duel Instance, and implement the [`RoomProvider`].
pub struct DuelHost {
    pub(crate) ctos_sender: mpsc::UnboundedSender<Request>,
    /// Get a signal that only sent once when duel ends.
    pub finished_sender: watch::Sender<bool>,
}

/// Failure to replace a Lua function through a duel host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaRegistrationError {
	/// The duel actor stopped before acknowledging the request.
	HostClosed,
	/// The core rejected registration.
	Core(ygopro_core_wrapper::lua::RegistrationError),
}

impl std::fmt::Display for LuaRegistrationError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Lua registration failed: {self:?}")
	}
}

impl std::error::Error for LuaRegistrationError {}

/// An opaque registration request constructed by [`DuelHost::register_lua_function`].
/// Private fields ensure that only an unsafe registration can supply callbacks.
pub struct LuaRegistrationRequest {
	table: String,
	name: String,
	callback: ygopro_core_wrapper::lua::Function,
	reply: tokio::sync::oneshot::Sender<Result<(), LuaRegistrationError>>,
}

impl LuaRegistrationRequest {
	pub(crate) fn apply(self, duel: &mut crate::duel::Duel) {
		// The host caller guarantees callback safety for the duel's lifetime.
		let result = unsafe {
			duel.core.register_lua_function(&self.table, &self.name, self.callback)
		}.map_err(LuaRegistrationError::Core);
		let _ = self.reply.send(result);
	}
}

impl DuelHost {
	/// Replace a Lua API on the duel actor and wait for registration to finish.
	///
	/// Call this before accepting clients / starting play. No Lua state pointer
	/// leaves the actor. An empty table name selects a global function.
	/// This affects the current core only: match siding recreates the core, so
	/// replacements must be registered again for the next duel. Existing Lua
	/// references to the original function are unaffected.
	///
	/// Dropping the future after enqueueing does not cancel registration.
	///
	/// # Example
	///
	/// ```no_run
	/// use ygopro::DuelHost;
	/// use ygopro_core_wrapper::lua;
	///
	/// unsafe extern "C" fn get_lp(state: *mut lua::State) -> std::ffi::c_int {
	/// 	unsafe { lua::push_integer(state, 12345) };
	/// 	1
	/// }
	///
	/// # async fn example(host: &DuelHost) -> Result<(), ygopro::host::LuaRegistrationError> {
	/// unsafe { host.register_lua_function("Duel", "GetLP", get_lp).await? };
	/// # Ok(())
	/// # }
	/// ```
	///
	/// # Safety
	/// The callback must satisfy all safety requirements of
	/// [`ygopro_core_wrapper::Duel::register_lua_function`], including no panic,
	/// Lua error, or yield through Rust frames. It executes on the actor's thread,
	/// which may differ from the caller's thread. Its code must remain loaded
	/// until the duel ends, even if this future is cancelled.
	pub async unsafe fn register_lua_function(&self, table: &str, name: &str,
		callback: ygopro_core_wrapper::lua::Function) -> Result<(), LuaRegistrationError> {
		let (reply, response) = tokio::sync::oneshot::channel();
		self.ctos_sender.send(Request::RegisterLuaFunction(LuaRegistrationRequest {
			table: table.to_owned(),
			name: name.to_owned(),
			callback,
			reply,
		})).map_err(|_| LuaRegistrationError::HostClosed)?;
		response.await.map_err(|_| LuaRegistrationError::HostClosed)?
	}

    /// Create a duel by target HostInfo and Configuration.
    pub fn new(host_info: HostInfo, configuration: Configuration) -> Self {
        let (request_sender, handle) = if host_info.mode == Mode::Tag {
            let tag_duel = TagDuel::new(host_info.clone(), configuration);
            let request_sender = tag_duel.request_sender.clone();
            let handle = tag_duel.run().expect("duel already started");
            (request_sender, handle)
        } else {
            let single_duel = SingleDuel::new(host_info.clone(), configuration);
            let request_sender = single_duel.request_sender.clone();
            let handle = single_duel.run().expect("duel already started");
            (request_sender, handle)
        };
        let (finished_sender, _) = watch::channel(false);
        let finished_sender_for_host = finished_sender.clone();
        tokio::spawn(async move {
            let _ = handle.await;
            finished_sender.send(true).ok();
        });
        request_sender.send(Request::Message(crate::ygopro_handlers::Request { message: ctos::Message::CreateGame(ctos::CreateGame { host_info, name: FixedLengthString::allocate(), pass: FixedLengthString::allocate() }), extra: Netplayer::Unknown })).ok();
        Self { ctos_sender: request_sender, finished_sender: finished_sender_for_host }
    }

    fn bridge<Item, Convert>(&self, client_to_server_stream: impl Stream<Item = Item> + Unpin + Send + 'static, mut convert: Convert) -> UnboundedReceiverStream<Complex<stoc::Message>> 
    where Item: Send + 'static, 
          Convert: FnMut(Item) -> Option<ctos::Message> + Send + 'static 
    {
        let ctos_sender = self.ctos_sender.clone();
        let (stoc_sender, stoc_receiver) = mpsc::unbounded_channel();
        let (return_sender, return_receiver) = mpsc::unbounded_channel();
        let (position_sender, position_receiver) = tokio::sync::oneshot::channel();
        ctos_sender.send(Request::MessageEx(crate::ygopro_handlers::RequestEx { message: crate::message::ClientJoin { stoc_sender, position_sender: Some(position_sender) }.into(), extra: SendTarget::None })).ok();

        tokio::spawn(async move {
            let mut ctos_stream = Box::pin(client_to_server_stream);
            let mut stoc_stream = UnboundedReceiverStream::new(stoc_receiver);
            let mut my_position = position_receiver.await.unwrap_or(Netplayer::Unknown);
            loop {
                tokio::select! {
                    message = ctos_stream.next() => {
                        match message {
                            Some(item) => if let Some(message) = convert(item) {
                                log::debug!("[←C {my_position:?}] {message:?}");
                                ctos_sender.send(Request::Message(crate::ygopro_handlers::Request { message, extra: my_position })).ok();
                            },
                            None => {
                                ctos_sender.send(Request::Message(crate::ygopro_handlers::Request { message: ctos::Message::LeaveGame(ctos::LeaveGame), extra: my_position })).ok();
                                break;
                            }
                        }
                    }
                    message = stoc_stream.next() => {
                        if let Some(message) = message {
                            match message.deref() {
                                stoc::Message::TypeChange(type_change) => my_position = type_change.player,
                                stoc::Message::LeaveGame(leave_game) => if leave_game.pos == my_position { break },
                                _ => ()
                            };
                            log::debug!("[S→ {my_position:?}] {:?}", message.deref());
                            return_sender.send(message).ok();
                        } else { break; }
                    }
                }
            }
        });
        UnboundedReceiverStream::new(return_receiver)
    }

    fn finish_signal(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let mut finished_receiver = self.finished_sender.subscribe();
        Box::pin(async move {
            let _ = finished_receiver.wait_for(|finished| *finished).await;
        })
    }
}

impl RoomProvider<ctos::Message, Complex<stoc::Message>> for DuelHost {
    type ServerToClientStream = UnboundedReceiverStream<Complex<stoc::Message>>;
    type FinishFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

    fn add(&mut self, client_to_server_stream: impl Stream<Item = ctos::Message> + Unpin + Send + 'static) -> Self::ServerToClientStream {
        self.bridge(client_to_server_stream, |message| Some(message))
    }

    fn get_finish_signal(&mut self) -> Self::FinishFuture {
        self.finish_signal()
    }
}

impl RoomProvider<Complex<ctos::Message>, Complex<stoc::Message>> for DuelHost {
    type ServerToClientStream = UnboundedReceiverStream<Complex<stoc::Message>>;
    type FinishFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

    fn add(&mut self, client_to_server_stream: impl Stream<Item = Complex<ctos::Message>> + Unpin + Send + 'static) -> Self::ServerToClientStream {
        self.bridge(client_to_server_stream, |complex| complex.into_inner())
    }

    fn get_finish_signal(&mut self) -> Self::FinishFuture {
        self.finish_signal()
    }
}
