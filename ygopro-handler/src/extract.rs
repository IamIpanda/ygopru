//! Message extraction and response construction.
//!
//! # `FromRequest` implementations
//!
//! | Req | Extracts | Where |
//! |-----|----------|-------|
//! | `Request<Message, Extra>` | `Extra` | `Extra: SocketAddr` |
//! | `Request<Message, Extra>` | `Extra` | `Extra: Netplayer` |
//! | `Request<Message, Extra>` | `Extra` | `Extra: CorePlayer` |
//! | `Request<Message, Extra>` | `&Message` | `Message: Send, Extra: Send` |
//! | `Request<ctos::Message, Extra>` | `&ctos::$variant` | generated per variant |
//! | `ctos::Message` | `&ctos::$variant` | generated per variant |
//! | `Request<stoc::Message, Extra>` | `&stoc::$variant` | generated per variant |
//! | `stoc::Message` | `&stoc::$variant` | generated per variant |
//! | `Request<Complex<ctos::Message>, Extra>` | `&ctos::$variant` | generated per variant |
//! | `Complex<ctos::Message>` | `&ctos::$variant` | generated per variant |
//! | `Request<Complex<stoc::Message>, Extra>` | `&stoc::$variant` | generated per variant |
//! | `Complex<stoc::Message>` | `&stoc::$variant` | generated per variant |
//! | `Request<gm::Message, Extra>` | `&gm::$variant` | generated per variant |
//! | `gm::Message` | `&gm::$variant` | generated per variant |
//! | `Request<Complex<gm::Message>, Extra>` | `&gm::$variant` | generated per variant |
//! | `Complex<gm::Message>` | `&gm::$variant` | generated per variant |
//!
//! # `IntoResponse` implementations
//!
//! | Type | Response |
//! |------|----------|
//! | `()` | `Continue` |
//! | `Infallible` | `Continue` |
//! | `Vec<Message>` | `ReplaceMultiple` |
//! | `bool` | `Stop` if true, `Continue` if false |
//! | `&'static str` | `"stop"` → Stop, `"kick"` → Kick, `"cancel"` → Swallow |
//! | `ctos::Message` | `Replace` with self |
//! | `stoc::Message` | `Replace` with self |
//! | `gm::Message` | `Replace` with self |
//! | `ctos::$variant` / `stoc::$variant` / `gm::$variant` | `Replace` with wrapped `Message::$variant` |
//!
//! # `MessageKey` implementations
//!
//! | Type | Key |
//! |------|-----|
//! | `Request<Message, Extra>` | delegates to `Message::message_key()` |

use std::convert::Infallible;
use std::net::SocketAddr;

use ygopro_data::complex::Complex;
use ygopro_data::constants::CorePlayer;
use ygopro_data::constants::Netplayer;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::message::gm;

use crate::IntoResponse;
use crate::handler::Bundle;
use crate::handler::FromRequest;

pub struct Request<Message, Extra> {
    pub message: Message,
    pub extra: Extra,
}

macro_rules! impl_extractable {
    ($extra:ty) => {
        impl<Message, State, Res> FromRequest<Request<Message, $extra>, State, Res> for $extra
        where
            Message: Send,
            State: Send,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<Request<Message, $extra>, State, Res>) -> Option<Self> {
                Some(bundle.request.extra)
            }
        }
    };
}

impl_extractable!(SocketAddr);
impl_extractable!(Netplayer);
impl_extractable!(CorePlayer);

impl<Message, Extra, State, Res> FromRequest<Request<Message, Extra>, State, Res> for &Message
where
    Message: Send,
    Extra: Send,
    State: Send,
    Res: Send,
{
    fn from_request(bundle: &mut Bundle<Request<Message, Extra>, State, Res>) -> Option<Self> {
        Some(unsafe { &*(&bundle.request.message as *const Message) })
    }
}

macro_rules! impl_variant_ref {
    ($message_mod:ident, $variant:ident) => {
        impl<Extra, State, Res> FromRequest<Request<$message_mod::Message, Extra>, State, Res> for &$message_mod::$variant
        where
            Extra: Send,
            State: Send,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<Request<$message_mod::Message, Extra>, State, Res>) -> Option<Self> {
                if let $message_mod::Message::$variant(inner) = &bundle.request.message {
                    Some(unsafe { &*(inner as *const $message_mod::$variant) })
                } else {
                    None
                }
            }
        }

        impl<State, Res> FromRequest<$message_mod::Message, State, Res> for &$message_mod::$variant
        where
            State: Send,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<$message_mod::Message, State, Res>) -> Option<Self> {
                if let $message_mod::Message::$variant(inner) = &bundle.request {
                    Some(unsafe { &*(inner as *const $message_mod::$variant) })
                } else {
                    None
                }
            }
        }
    };
}

macro_rules! impl_variant_complex_ref {
    ($message_mod:ident, $variant:ident) => {
        impl<Extra, State, Res> FromRequest<Request<Complex<$message_mod::Message>, Extra>, State, Res> for &$message_mod::$variant
        where
            Extra: Send,
            State: Send,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<Request<Complex<$message_mod::Message>, Extra>, State, Res>) -> Option<Self> {
                if let $message_mod::Message::$variant(inner) = &*bundle.request.message {
                    Some(unsafe { &*std::ptr::from_ref(inner) })
                } else {
                    None
                }
            }
        }

        impl<State, Res> FromRequest<Complex<$message_mod::Message>, State, Res> for &$message_mod::$variant
        where
            State: Send,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<Complex<$message_mod::Message>, State, Res>) -> Option<Self> {
                if let $message_mod::Message::$variant(inner) = &*bundle.request {
                    Some(unsafe { &*std::ptr::from_ref(inner) })
                } else {
                    None
                }
            }
        }
    };
}

macro_rules! impl_ctos {
    ($($variant:ident = $flag:literal),* $(,)?) => {
        $( impl_variant_ref!(ctos, $variant); )*
        $( impl_variant_complex_ref!(ctos, $variant); )*
        $( impl_variant_response!(ctos, $variant); )*
    };
}

macro_rules! impl_stoc {
    ($($variant:ident = $flag:literal),* $(,)?) => {
        $( impl_variant_ref!(stoc, $variant); )*
        $( impl_variant_complex_ref!(stoc, $variant); )*
        $( impl_variant_response!(stoc, $variant); )*
    };
}

macro_rules! impl_gm {
    ($($variant:ident = $flag:literal),* $(,)?) => {
        $( impl_variant_ref!(gm, $variant); )*
        $( impl_variant_complex_ref!(gm, $variant); )*
        $( impl_variant_response!(gm, $variant); )*
    };
}

pub enum Response<Message> {
    /// Continue processing the message as normal.
    Continue,
    /// Message will be replaced with the given message when sending to its target.
    Replace(Message),
    /// Message will be replaced with multiple messages when sending to its target.
    ReplaceMultiple(Vec<Message>),
    /// This message will not send to its target.
    Swallow,
    /// This message will not send to its target, and stop processing further handlers.
    Stop,
    /// This message will not send to its target, and kick its source.
    Kick,
}

impl<Message> Default for Response<Message> {
    fn default() -> Self {
        Response::Continue
    }
}

impl IntoResponse<Response<ctos::Message>> for ctos::Message {
    fn into_response(self) -> Response<ctos::Message> {
        Response::Replace(self)
    }
}

impl IntoResponse<Response<stoc::Message>> for stoc::Message {
    fn into_response(self) -> Response<stoc::Message> {
        Response::Replace(self)
    }
}

impl IntoResponse<Response<gm::Message>> for gm::Message {
    fn into_response(self) -> Response<gm::Message> {
        Response::Replace(self)
    }
}

macro_rules! impl_variant_response {
    ($message_mod:ident, $variant:ident) => {
        impl IntoResponse<Response<$message_mod::Message>> for $message_mod::$variant {
            fn into_response(self) -> Response<$message_mod::Message> {
                Response::Replace($message_mod::Message::$variant(self))
            }
        }
    };
}

impl<Message> IntoResponse<Response<Message>> for () {
    fn into_response(self) -> Response<Message> {
        Response::Continue
    }
}

impl<Message> IntoResponse<Response<Message>> for Infallible {
    fn into_response(self) -> Response<Message> {
        Response::Continue
    }
}

impl<Message> IntoResponse<Response<Message>> for Vec<Message> {
    fn into_response(self) -> Response<Message> {
        Response::ReplaceMultiple(self)
    }
}

impl<Message> IntoResponse<Response<Message>> for bool {
    fn into_response(self) -> Response<Message> {
        if self { Response::Stop } else { Response::Continue }
    }
}

impl<Message> IntoResponse<Response<Message>> for &'static str {
    fn into_response(self) -> Response<Message> {
        match self {
            "continue" => Response::Continue,
            "stop" => Response::Stop,
            "kick" => Response::Kick,
            "cancel" | "_cancel" => Response::Swallow,
            _ => Response::Continue,
        }
    }
}

impl <Message> IntoResponse<Response<Message>> for Option<Message> {
    fn into_response(self) -> Response<Message> {
        match self {
            Some(message) => Response::Replace(message),
            None => Response::Continue,
        }
    }
}

impl<Message, Response1, Response2> IntoResponse<Response<Message>> for Result<Response1, Response2>
where Response1: IntoResponse<Response<Message>>, Response2: IntoResponse<Response<Message>> {
    fn into_response(self) -> Response<Message> {
        match self {
            Ok(response1) => response1.into_response(),
            Err(response2) => response2.into_response(),
        }
    }
}

ygopro_data::every_client_to_server_flat_message!(impl_ctos);
ygopro_data::every_server_to_client_flat_message!(impl_stoc);
ygopro_data::every_game_message_flat_message!(impl_gm);
