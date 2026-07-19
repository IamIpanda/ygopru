use std::convert::Infallible;
use std::net::SocketAddr;

use ygopro_data::complex::Complex;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;

use crate::IntoResponse;
use crate::handler::Bundle;
use crate::handler::FromRequest;

pub struct Request<Message> {
    pub message: Message,
    pub client_addr: SocketAddr,
}

impl<Message, State, Res> FromRequest<Request<Message>, State, Res> for SocketAddr
where
    Message: Send,
    State: Send + Sync,
    Res: Send,
{
    fn from_request(bundle: &mut Bundle<Request<Message>, State, Res>) -> Option<Self> {
        Some(bundle.request.client_addr)
    }
}

impl<Message, State, Res> FromRequest<Request<Message>, State, Res> for &Message
where
    Message: Send,
    State: Send + Sync,
    Res: Send,
{
    fn from_request(bundle: &mut Bundle<Request<Message>, State, Res>) -> Option<Self> {
        Some(unsafe { &*(&bundle.request.message as *const Message) })
    }
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
    Kick
}

impl<Message> Default for Response<Message> {
    fn default() -> Self {
        Response::Continue
    }
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
            _ => Response::Continue
        }
    }
}

impl IntoResponse<Response<stoc::Message>> for stoc::Message {
    fn into_response(self) -> Response<stoc::Message> {
        Response::Replace(self)
    }
}

impl IntoResponse<Response<ctos::Message>> for ctos::Message {
    fn into_response(self) -> Response<ctos::Message> {
        Response::Replace(self)
    }
}

macro_rules! impl_variant_ref {
    ($message_mod:ident, $variant:ident) => {
        impl<State, Res> FromRequest<Request<$message_mod::Message>, State, Res> for &$message_mod::$variant
        where
            State: Send + Sync,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<Request<$message_mod::Message>, State, Res>) -> Option<Self> {
                if let $message_mod::Message::$variant(inner) = &bundle.request.message {
                    Some(unsafe { &*(inner as *const $message_mod::$variant) })
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

macro_rules! impl_variant_complex_ref {
    ($message_mod:ident, $variant:ident) => {
        impl<State, Res> FromRequest<Request<Complex<$message_mod::Message>>, State, Res> for &$message_mod::$variant
        where
            State: Send + Sync,
            Res: Send,
        {
            fn from_request(bundle: &mut Bundle<Request<Complex<$message_mod::Message>>, State, Res>) -> Option<Self> {
                if let $message_mod::Message::$variant(inner) = &*bundle.request.message {
                    Some(unsafe { &*std::ptr::from_ref(inner) })
                } else {
                    None
                }
            }
        }
    };
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

ygopro_data::every_client_to_server_flat_message!(impl_ctos);
ygopro_data::every_server_to_client_flat_message!(impl_stoc);
