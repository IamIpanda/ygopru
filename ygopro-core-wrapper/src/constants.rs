//! Constants that is useful in Duel.

/// Max length of a [`gm::Message`](ygopro_data::message::gm::Message).
pub const SIZE_MESSAGE_BUFFER: usize = 0x2000;
/// Max length if a [`ctos::Response`](ygopro_data::message::ctos::Response).
pub const SIZE_RETURN_VALUE: usize = 512;
/// Max length of an AI name (not used for now.)
pub const SIZE_AI_NAME: usize = 128;
/// Max length of a [`gm::Hint`](ygopro_data::message::gm::Hint).
pub const SIZE_HINT_MSG: usize = 1024;
/// Max length of a response of [`Query`](ygopro_data::constants::Query).
///
/// See also: [`query_card`](super::query_card), [`query_field_count`](super::query_field_count),
/// [`query_field_card`](super::query_field_card), [`query_field_info`](super::query_field_info)
pub const SIZE_QUERY_BUFFER: usize = 0x40000;
