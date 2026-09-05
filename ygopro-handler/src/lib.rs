//! A type-erased, plugin-based message handler framework.
//!
//! This crate provides the machinery to dispatch messages to registered handlers,
//! extract request data, and combine handler responses. Every incoming message flows
//! through a [`Processor`] and is handled by a set of handlers keyed by message type.
//!
//! The extractor/handler pattern mirrors [`axum`](https://docs.rs/axum): handlers are
//! plain functions whose parameters are pulled from a [`Bundle`] via [`FromRequest`],
//! and whose return value is converted into a response via [`IntoResponse`]. Any type
//! implementing [`FromRequest`] can be a parameter, and any type implementing
//! [`IntoResponse`] can be returned — exactly like axum's
//! [`FromRequest`](https://docs.rs/axum/latest/axum/extract/trait.FromRequest.html) and
//! [`IntoResponse`](https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html).
//!
//! Unlike axum, however, a message is not handled by a single handler but by a **chain**
//! of handlers (all those registered for the message key, plus the globals), and the
//! chain can be **aborted** mid-way via [`StopFlag`]. This makes it easy to layer
//! plugins: each plugin registers its own handlers for a message, and any one of them
//! can stop the chain or replace the message that the rest see.
//!
//! # Design goals
//!
//! - **Handlers as closures.** A handler is any callable that takes extractable
//!   parameters and returns a value convertible into a [`extract::Response`]. Up to
//!   16 parameters are supported; each is pulled from a [`Bundle`] via [`FromRequest`].
//! - **Message key dispatch.** Each message carries a [`MessageKey`] (usually a `u8`
//!   flag). The [`Processor`] routes a message to the handlers registered for that
//!   key, plus any global handlers.
//! - **Chained, abortable processing.** A message flows through every handler registered
//!   for its key, plus the globals, in priority order. Any handler can abort the chain
//!   via [`StopFlag`] or replace the message that downstream handlers see. Their outputs
//!   are combined through [`std::ops::Mul`] on the response, letting one handler replace
//!   the message, another swallow it, and another terminate the room.
//! - **Type-erased handlers.** [`SyncHandler`], [`AsyncHandler`] and [`TowerHandler`]
//!   erase the concrete handler type so a heterogeneous set of handlers can live in
//!   a single [`Processor`].
//!
//! # Soundness
//!
//! This crate is **unsafe**. You must carefully handle the parameters to prevent
//! undefined behaviour, especially **dual mutable references** — a handler that takes
//! two `&mut` parameters can alias the same memory. The extraction uses raw-pointer
//! casts because the Rust compiler cannot properly calculate the required lifetimes, 
//! and due to the architecture the crate cannot offer the same guarantees that axum's
//! `FromRequest` provides.
//!
//! # Performance
//!
//! The crate offers three handler wrappers, trading features against speed:
//!
//! 1. [`TowerHandler`] — the most feature-complete, and the slowest. Between the handler
//!    and the processor it inserts a tower `Service` layer (`HandlerService`), a boxed
//!    future (`HandlerServiceFuture`), and a `BoxCloneService`, so every call pays for
//!    several layers of boxing and a `oneshot` dispatch.
//! 2. [`AsyncHandler`] — gives up the tower adaptation layer. It holds the handler in an
//!    `Arc<dyn Call>` and boxes only the future, so it drops the extra `Service` boxing
//!    while staying cloneable through a cheap `Arc` clone.
//! 3. [`SyncHandler`] — gives up the async nature of [`Handler`]: its future is always
//!    `Ready`. In exchange it boxes nothing per call, drops the `'static` bound on the
//!    request, state, and response, and enables the dual-state trick
//!    ([`handler::sync_handler::WithSubState`]).
//!
//! Compared with the C++ original, which dispatches messages with a direct switch or a
//! virtual call, every handler here pays for at least one heap-allocated future and a
//! type-erasure indirection (a trait object or a function pointer) per invocation.
//! [`Processor::process`] additionally awaits the whole handler chain per item, moving
//! the [`Bundle`] through it. For the hot path (game messages) this is one boxed future
//! per handler, which is acceptable at the rate messages are emitted.
//!
//! # Example
//!
//! A processor dispatches a message to the handlers registered for its key:
//!
//! ```
//! use ygopro_handler::Processor;
//! use ygopro_handler::TowerHandler;
//! use ygopro_handler::Bundle;
//! use ygopro_handler::State;
//! use ygopro_handler::extract::Request;
//! use ygopro_handler::extract::Response;
//!
//! type Req = Request<u8, ()>;
//! type Res = Response<u8>;
//!
//! let mut processor = Processor::<u8, Req, State, Res>::new();
//! processor.register(7, TowerHandler::new(0, "test", "example", |message: &u8| -> Res {
//!     if *message == 7 {
//!         Res::Replace(*message)
//!     } else {
//!         Res::Continue
//!     }
//! }));
//!
//! let bundle = Bundle::new(Request { message: 7, extra: () }, State::new(), Res::Continue);
//! let result = tokio::runtime::Runtime::new().unwrap().block_on(processor.process_bundle(bundle, 7));
//! assert!(matches!(result.response, Res::Replace(7)));
//! ```

#![warn(missing_docs)]

mod room;
pub mod extract;
pub mod handler;
pub mod processor;

pub use room::*;
pub use handler::*;
pub use handler::tower_handler::TowerHandler;
pub use handler::async_handler::AsyncHandler;
pub use handler::sync_handler::SyncHandler;
pub use processor::*;
