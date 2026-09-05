//! Procedural macros for the ygopro ecosystem, powering `ygopro-data`,
//! `ygopro-handler` and `ygopro`.
//! 
//! Most of macros used by `srvpro` also here because I'm lazy.
//!
//! The macros are grouped by what they do:
//!
//! | group | macros | purpose |
//! | ----- | ------ | ------- |
//! | protocol data | [`crate::Message`], [`crate::GameMessage`] | map structs to wire types; generate masking logic for game messages |
//! | plugin state | [`crate::Attachment`], [`crate::Configuration`] | per-duel shared state and environment-driven plugin configuration |
//! | handling | [`crate::handler`], [`crate::before`], [`crate::after`], [`crate::command`] | turn functions into message/command handlers |
//! | registration | [`crate::register_to`] | put handlers into [`linkme`](https://docs.rs/linkme) distributed slices at link time |
//!
//! # Notes
//!
//! - Macros emit code that resolves against `ygopro_data` and
//!   `ygopro_handler`; using them outside the ygopro workspace requires both
//!   crates as dependencies.
//! - The generated `FromRequest` impls rely on raw-pointer casts that are
//!   sound only under `ygopro-handler`'s layout guarantees; see that crate's
//!   soundness section before taking `&mut` of the same state twice.

use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::AttributeArgs;
use syn::ItemFn;

mod attachment;
mod command;
mod configuration;
mod mask;
mod message;
mod registry;


/// Attaches plugin state to a state's anymap.
///
/// An attachment is a parasite living inside the anymap: it has no fixed
/// field in any struct, but rides on an anymap slot that a struct (e.g. the
/// duel state) deliberately reserves. This lets every plugin carry its own
/// extra information on the same reserved map without touching the host
/// struct, so hosts stay generic and plugins stay additive.
///
/// The instance is looked up in the state's anymap (through
/// `ContainsMapMut`). When missing, a default instance is built and inserted
/// on first use; with `no_default`, extraction fails and the handler is
/// skipped instead.
///
/// This derive implements `FromRequest` for `&mut Struct`, so the struct can be a
/// handler parameter holding plugin state shared by every handler in the
/// chain.
///
/// # The `#[attachment(...)]` attribute
///
/// On the struct:
///
/// - `no_default` — never create the instance on demand; the handler is
///   skipped when the attachment is absent.
///
/// On a field:
///
/// - `default = "<expr>"` — the default value expression; falls back to
///   `Default::default()` when omitted.
///
/// # Generated code
///
/// A `default_attachment()` constructor with each field defaulted (skipped
/// for `no_default`), plus:
///
/// ```text
/// impl<Req, State, Res> FromRequest<Req, State, Res> for &mut Attachment
/// where State: ContainsMapMut
/// {
///     fn from_request(bundle: &mut Bundle<Req, State, Res>) -> Option<Self> {
///         let map = ContainsMapMut::get_map(&mut bundle.state);
///         let attachment = map.entry::<Attachment>().or_insert_with(Attachment::default_attachment);
///         Some(unsafe { &mut *(attachment as *mut Attachment) })
///     }
/// }
/// ```
///
/// # Examples
///
/// `ygopro/src/plugin/reconnect.rs` keeps per-duel reconnect state as an
/// attachment:
///
/// ```text
/// #[derive(Attachment)]
/// pub struct Attachment {
///     #[attachment(default = "Phase::Draw")]
///     phase: Phase,
///     deck_reversed: bool,
///     #[attachment(default = "CorePlayer::None")]
///     turn_player: CorePlayer,
/// }
///
/// #[after(gm::NewPhase)]
/// #[register_to(YGOCORE_HANDLERS as YgocoreHandler)]
/// fn on_new_phase(attachment: &mut Attachment, message: &gm::NewPhase) {
///     attachment.phase = message.phase;
/// }
/// ```
#[proc_macro_derive(Attachment, attributes(attachment))]
pub fn attachment(input: TokenStream) -> TokenStream {
    attachment::attachment(input)
}

/// Generates a plugin configuration with environment overrides and
/// link-time registration.
///
/// It generates default values, environment variable overrides, insertion
/// into the configuration map, link-time registration into a distributed
/// slice, and a `FromRequest` extractor.
///
/// The struct must implement `Clone` so the `FromRequest` impl can hand out
/// copies; combine with `#[derive(Clone)]`.
///
/// # The `#[config(...)]` attribute
///
/// On the struct:
///
/// - `register_to = "<path>"` — the distributed slice the configuration is
///   registered into; defaults to `crate::plugin::CONFIGURATIONS`.
/// - `sync` — store the value in a `dyn CloneAny + Send + Sync` map instead of
///   `dyn CloneAny + Send`.
/// - `prefix = "<str>"` — prefix of the environment variable names; defaults
///   to the last segment of `module_path!()`; an empty string keeps the bare
///   field names.
///
/// On a field:
///
/// - `not_from_env` — skip the environment override for this field.
/// - `default = "<expr>"` — the default value expression; falls back to
///   `Default::default()` when omitted.
///
/// # Environment overrides
///
/// For each overridable field, the generated code asks the config manager for
/// `{prefix}_{field}` (lowercase) and parses the value into the field:
///
/// ```text
/// configuration.terminate_when_match_end = config_manager
///     .get("ygopro::plugin::terminate_terminate_when_match_end")
///     .and_then(|value| value.parse().ok())
///     .unwrap_or(configuration.terminate_when_match_end);
/// ```
///
/// # Generated registration
///
/// ```text
/// #[linkme::distributed_slice(crate::plugin::CONFIGURATIONS)]
/// static CONFIGURATION: (&'static str, fn(&mut anymap3::Map<dyn CloneAny + Send>) -> Result<(), Box<dyn Error>>) =
///     (module_path!(), Configuration::default_configuration);
/// ```
///
/// # Example
///
/// `ygopro/src/plugin/terminate.rs`:
///
/// ```text
/// #[derive(Clone, Configuration)]
/// pub struct Configuration {
///     #[config(not_from_env)]
///     pub terminate_when: SendTarget,
///     #[config(default = "true")]
///     pub terminate_when_match_end: bool,
/// }
/// ```
#[proc_macro_derive(Configuration, attributes(config))]
pub fn configuration(input: TokenStream) -> TokenStream {
    configuration::configuration(input)
}

/// Maps a struct to its protocol message type.
///
/// Implements `ygopro_data::message::PureMessage` and
/// `ygopro_data::message::Message` for the struct, mapping it to its
/// protocol type via `message_type()`.
///
/// # The `#[message(...)]` attribute
///
/// - `direction` — required path: `ctos`, `stoc`, `gm`, or a custom
///   identifier that is stored verbatim as a `&'static str` tag.
/// - `flag = <u8>` — protocol byte of the message.
/// - `mod_name = "<ident>"` — path head of the generated impls.
///
/// ```text
/// #[derive(BinRead, BinWrite, Debug, Clone, Message)]
/// #[message(ctos, flag = 17)]
/// pub struct CreateGame {
///     pub host_info: HostInfo,
///     pub name: FixedLengthString<20>,
///     pub pass: FixedLengthString<20>,
/// }
/// ```
///
/// ## Directions
///
/// - `ctos` — generates
///   `MessageType::CTOS(client_to_server::MessageType::StructName)`.
/// - `stoc` — generates
///   `MessageType::STOC(server_to_client::MessageType::StructName)`.
/// - `gm` — generates
///   `MessageType::GM(game_message::MessageType::StructName)`.
/// - any other identifier — generates `MessageType::Other(tag, flag)`; requires
///   `flag` or fails with "Don't offer a flag".
///
/// The struct name must exactly match the variant name in the direction's
/// `MessageType` enum, since the derive refers to the variant by struct ident.
///
/// A custom direction is how the `ygopro` crate defines internal
/// duel-actor messages that never cross the wire. Those carry the `ygopro`
/// tag and only live inside the actor (see `ygopro/src/message.rs`):
///
/// ```text
/// #[derive(Debug, Message)]
/// #[message(ygopro, flag = 1)]
/// pub struct ClientJoin {
///     pub stoc_sender: mpsc::UnboundedSender<Complex<message::stoc::Message>>,
///     pub position_sender: Option<tokio::sync::oneshot::Sender<Netplayer>>,
/// }
/// ```
///
/// ## `flag`
///
/// For standard directions the derive itself ignores `flag`; it is consumed
/// by `ygopro-data`'s `build.rs`, which scans the attribute to generate the
/// discriminant table for `every_xxx_flat_message!`.
///
/// ## `mod_name`
///
/// Defaults to `crate` when the containing crate is `ygopro-data`, otherwise
/// `::ygopro_data`. Override for renamed dependencies; must parse as a single
/// identifier.
///
/// # Scope
///
/// The derive only maps to `message_type()`. Wire serialization is the job of
/// `BinRead`/`BinWrite`; no parsing or dispatch registration happens here.
#[proc_macro_derive(Message, attributes(message))]
pub fn ygopro_message(input: TokenStream) -> TokenStream {
    message::ygopro_message(input)
}

/// Implements per-player masking and waiting logic that only game messages need.
///
/// Game messages are broadcast to every player, and unlike ctos/stoc messages
/// they carry private info (opponent's hand, deck, face-down cards, ...) that
/// must be hidden from the players it does not belong to, and some of them
/// wait for a specific player's input. This derive generates both: [`crate::mask`]
/// wipes the fields that must be hidden, `should_mask(player)` decides whether
/// the message needs masking for that player at all, and `waiting_for()`
/// reports the player whose input the message waits for.
///
/// # Masking
///
/// The `#[mask]` and `#[mask_if]` attributes on a field:
///
/// - `#[mask]` — wipe the field in `mask()`. A primitive field is reset to
///   `Default::default()`; any other type is wiped via its own `mask()`.
/// - `#[mask(<expr>)]` — replace the field with the given expression instead.
/// - `#[mask_if(<cond>)]` — the condition is OR-ed into `should_mask(player)`;
///   a field with only `mask_if` still gets `Default::default()` applied.
///
/// `should_mask` is generated only when needed:
///
/// - any unconditional `#[mask]` → always `true`;
/// - only `#[mask_if]` → the OR of all conditions;
/// - no masking → omitted, falling back to the trait's default (`true`).
///
/// # Waiting for input
///
/// The `#[wait_for]` attribute on a field marks the message as waiting for
/// that player's input. The field must be a `CorePlayer`; only the first
/// annotated field is used, and `waiting_for()` returns `Some(player)`.
///
/// This is what makes game messages different from ctos/stoc ones: the core
/// emits a game message and then stops until the player answers (a choice, a
/// response, ...). The handler chain consumes it — e.g.
/// `ygocore_handlers::on_all_message` routes `waiting_for()` to
/// `SendTarget::Core`, and the evolve loop stops when the last message waits
/// for input.
///
/// # Example
///
/// `UpdateData` hides the whole payload from the opponent; `Move` keeps the
/// card code only when it is public; `SelectBattleCommand` waits for the
/// selecting player (see `ygopro-data/src/message/game_message.rs`):
///
/// ```text
/// #[derive(Debug, Clone, Message, GameMessage)]
/// #[message(gm, flag = 6)]
/// pub struct UpdateData {
///     pub player: CorePlayer,
///     pub location: Location,
///     #[mask]
///     #[mask_if(self.player != player)]
///     pub data: Vec<UpdateCardInfo>,
/// }
///
/// #[derive(Debug, Clone, Message, GameMessage)]
/// #[message(gm, flag = 50)]
/// pub struct Move {
///     #[mask(if self.should_mask(CorePlayer::None) { 0 } else { self.code })]
///     #[mask_if(self.current.controller != player && ...)]
///     pub code: i32,
///     ...
/// }
///
/// #[derive(Debug, Clone, Message, GameMessage)]
/// #[message(gm, flag = 10)]
/// pub struct SelectBattleCommand {
///     #[wait_for]
///     pub selecting_player: CorePlayer,
///     ...
/// }
/// ```
#[proc_macro_derive(GameMessage, attributes(mask, mask_if, wait_for))]
pub fn mask(input: TokenStream) -> TokenStream {
    mask::mask(input)
}


/// Turns a function into a message handler and registers it into a
/// distributed slice via [`register_to`].
///
/// The function parameters are extracted from the `ygopro_handler::Bundle`
/// through `FromRequest`, and its return value is combined into the handler
/// chain's response. The function must be synchronous: it is wrapped in
/// `SyncHandler`, so no async functions are allowed.
///
/// # Syntax
///
/// - `key` — required path of a type implementing
///   `ygopro_data::message::Message`, e.g. `ctos::Response`, `gm::NewTurn`,
///   `ygopro::DuelStart`. The special `ygopro_handler::All` key registers the
///   handler globally, so it runs for every message.
/// - `priority = <u8>` — chain position; lower runs first. Default `128`.
/// - `module = "<path>"` — module name recorded on the handler; defaults to
///   `module_path!()`.
///
/// The handler must be paired with [`register_to`], otherwise the macro fails
/// with "no #[register_to] attribute found on handler".
///
/// # Generated code
///
/// For each `#[register_to(slice, as Type, with KeyType)]` annotation, the
/// macro emits a builder that converts the message key and wraps the function:
///
/// ```text
/// fn build_handle_on_response_handler() -> (u8, Handler) {
///     (
///         ::std::convert::Into::<u8>::into(
///             <ctos::Response as ::ygopro_data::message::Message>::message_type(),
///         ),
///         Handler::new(128, "on_response", module_path!(), on_response),
///     )
/// }
/// ```
///
/// The real registration into the slice is done by the [`register_to`]
/// attribute, which references this builder.
///
/// # Examples
///
/// A plain handler: parameters are pulled from the bundle in declaration
/// order, the return value is merged into the chain response. The slice and
/// handler type resolve against aliases declared in the same module
/// (`Request`, `State`, `Handler` in `ygopro/src/ygopro_handlers.rs`):
///
/// ```text
/// #[handler(ctos::Response)]
/// #[register_to(YGOPRO_HANDLERS)]
/// fn on_response(duel: &mut Duel, player: PlayerIndex, response: &ctos::Response) {
///     if duel.ended { return; }
///     duel.client_responses.push(response.clone());
///     duel.request_sender.send(crate::duel::Request::Evolve).ok();
/// }
/// ```
///
/// The key defaults to `u8`, so a slice with `[fn() -> (u8, Handler)]` is
/// registered as-is. When the slice uses a different key or handler type,
/// spell it out with `as` / `with`:
///
/// ```text
/// #[command]
/// #[register_to(crate::command::COMMANDS as crate::command::CommandHandler with &'static str)]
/// fn timer_tick(duel: &mut Duel, attachment: &mut TimeLimit) -> &'static str {
///     // ...
/// }
/// ```
///
/// A handler may carry several `#[register_to]` annotations; each one gets its
/// own builder and slice entry, so one function can serve multiple slices
/// with different key and handler types.
///
/// Register globally by keying on `ygopro_handler::All`; the handler then
/// runs for every message of that processor:
///
/// ```text
/// #[before(ygopro_handler::All)]
/// #[register_to(YGOCORE_HANDLERS)]
/// fn on_all_message(message: &gm::Message) -> SendTarget {
///     message.waiting_for().map(SendTarget::Core).unwrap_or(SendTarget::All)
/// }
/// ```
///
/// `before`/`after` shift the default priority, so they run earlier or later
/// than plain `handler`s on the same key. See `ygopro/src/plugin/time_limit.rs`
/// for a plugin that layers `after` handlers over the built-in ones:
///
/// ```text
/// #[after(ctos::CreateGame)]
/// #[register_to(YGOPRO_HANDLERS as YgoproHandler)]
/// fn on_create_game(duel: &mut Duel, attachment: &mut TimeLimit) {
///     duel.last_response = None;
///     attachment.time_elapsed = 0;
/// }
/// ```
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item, |priority| priority.unwrap_or(128))
}

/// Same as [`handler`], but registered to run before normal handlers.
///
/// The chain is ordered by ascending priority: a `before` handler gets
/// `128 - priority` (default `127`), so it runs before every `handler`
/// (default `128`).
///
/// # See also
///
/// - [`handler`] — the default chain position.
/// - [`after`] — runs after normal handlers.
#[proc_macro_attribute]
pub fn before(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item, |priority| 128u8 - priority.unwrap_or(1))
}

/// Same as [`handler`], but registered to run after normal handlers.
///
/// The chain is ordered by ascending priority: an `after` handler gets
/// `128 + priority` (default `129`), so it runs after every `handler`
/// (default `128`).
///
/// # See also
///
/// - [`handler`] — the default chain position.
/// - [`before`] — runs before normal handlers.
#[proc_macro_attribute]
pub fn after(attr: TokenStream, item: TokenStream) -> TokenStream {
    dispatch(attr, item, |priority| 128u8 + priority.unwrap_or(1))
}

/// Turns a function into a command handler, keyed by the function name.
///
/// A command is like a handler, but instead of a protocol message it is
/// triggered by a name: `duel.queue_command("my_command", payload)` looks up
/// the handler registered for that name. The command name is the function
/// name itself, so no key argument is accepted; the priority is always `128`.
///
/// Like the handler macros, `#[command]` must be stacked with
/// [`register_to`], which names the target slice. The slice is expected to
/// use `&'static str` as key — spell it with `with` (see
/// [`register_to`](#syntax)):
///
/// ```text
/// #[command]
/// #[register_to(crate::command::COMMANDS as crate::command::CommandHandler with &'static str)]
/// fn timer_tick(duel: &mut Duel, attachment: &mut TimeLimit) -> &'static str {
///     // ...
/// }
/// ```
///
/// Without `#[register_to]` the macro fails with "no #[register_to] attribute
/// found on command".
///
/// # Generated code
///
/// ```text
/// fn build_handle_timer_tick_commandhandler() -> (&'static str, CommandHandler) {
///     ("timer_tick", CommandHandler::new(128, "timer_tick", module_path!(), timer_tick))
/// }
/// ```
#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _ = attr;
    let function = parse_macro_input!(item as ItemFn);
    command::command_impl(function).into()
}

fn dispatch(attr: TokenStream, item: TokenStream, transform_priority: impl Fn(Option<u8>) -> u8) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let function = parse_macro_input!(item as ItemFn);
    let args = match registry::parse_args(&attr, transform_priority) {
        Ok(args) => args,
        Err(err) => return err.write_errors().into(),
    };
    registry::shared_impl(args, function).into()
}

/// Registers a handler function into a [`linkme`](https://docs.rs/linkme)
/// distributed slice.
///
/// The macro is meant to be stacked with [`handler`], [`before`], [`after`] or
/// [`command`]: those macros emit a `build_handle_<name>_<suffix>` builder
/// function, and this one references it from a `#[distributed_slice]` static,
/// so every registered handler is collected into the slice at link time.
///
/// # Syntax
///
/// - `slice` — required path of a `#[distributed_slice]` static declared as
///   `[fn() -> (Key, Handler)]`, e.g. `YGOPRO_HANDLERS`.
/// - `as <Type>` — handler type; defaults to `Handler`, so a file-local
///   alias named `Handler` must exist.
/// - `with <KeyType>` — key type; defaults to `u8`.
///
/// # Generated code
///
/// ```text
/// #[linkme::distributed_slice(YGOPRO_HANDLERS)]
/// static REGISTER_ON_RESPONSE_HANDLER: fn() -> (u8, Handler) =
///     build_handle_on_response_handler;
/// ```
///
/// The static name is derived from the function and handler type
/// (`REGISTER_<FN>_<SUFFIX>`, all uppercase).
///
/// # Examples
///
/// Default key and handler type, used with `#[handler]`:
///
/// ```text
/// #[handler(ctos::Response)]
/// #[register_to(YGOPRO_HANDLERS)]
/// fn on_response(duel: &mut Duel, player: PlayerIndex, response: &ctos::Response) {
///     // ...
/// }
/// ```
///
/// Explicit types, used with `#[command]`:
///
/// ```text
/// #[command]
/// #[register_to(crate::command::COMMANDS as crate::command::CommandHandler with &'static str)]
/// fn timer_tick(duel: &mut Duel, attachment: &mut TimeLimit) -> &'static str {
///     // ...
/// }
/// ```
///
/// One function may carry several `#[register_to]` annotations to enter
/// multiple slices, each with its own key/handler types.
#[proc_macro_attribute]
pub fn register_to(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: proc_macro2::TokenStream = attr.into();
    let info = match registry::parse_register_info(attr2) {
        Ok(info) => info,
        Err(err) => return err.to_compile_error().into(),
    };
    let function = parse_macro_input!(item as ItemFn);
    registry::register_to_impl(info, function).into()
}

