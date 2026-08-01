//! Validate a replay file by replaying it through a fresh `SingleDuelHost`.
//!
//! A `.yrp` file only contains the responses the players made, not the game
//! message stream. Reproducing the duel requires the same deck order and the
//! same random seed, both of which the replay records. This module opens a
//! `SingleDuelHost` as a `RoomProvider`, drives two client sessions against it,
//! feeds every recorded response back at the moment the engine asks for it, and
//! reports whether the duel finishes with a win and without a single rejected
//! response.

use std::collections::VecDeque;
use std::io::Cursor;
use std::ops::Deref;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use binrw::BinRead;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use ygopro::single_duel::Configuration;
use ygopro::single_duel::SingleDuelHost;
use ygopro_core_wrapper::random::SEED_COUNT;
use ygopro_core_wrapper::DuelSeed;
use ygopro_data::complex::Complex;
use ygopro_data::constants::CorePlayer;
use ygopro_data::constants::Hand;
use ygopro_data::constants::Netplayer;
use ygopro_data::constants::PlayerChangeState;
use ygopro_data::data::Deck;
use ygopro_data::data::Replay;
use ygopro_data::data::ReplayDeck;
use ygopro_data::data::ReplayMode;
use ygopro_data::message::ctos;
use ygopro_data::message::gm;
use ygopro_data::message::gm::GameMessage;
use ygopro_data::message::stoc;
use ygopro_data::string::FixedLengthString;
use ygopro_handler::RoomProvider;

const VALIDATION_TIMEOUT: Duration = Duration::from_secs(120);

static REPLAY_SEED: OnceLock<[u32; SEED_COUNT]> = OnceLock::new();

fn seed_generator(_duel_count: u8) -> DuelSeed {
    DuelSeed::Complicated(*REPLAY_SEED.get().expect("replay seed is not initialized"))
}

#[derive(Debug)]
pub enum ValidationError {
    Io(std::io::Error),
    Parse(binrw::Error),
    TagReplayNotSupported,
    EmptyReplay,
    Retry,
    TruncatedReplay,
    LeftoverResponses(usize),
    ServerRejected,
    NoWin,
    DuelDidNotEnd,
    Timeout,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Io(error) => write!(formatter, "cannot read replay file: {error}"),
            ValidationError::Parse(error) => write!(formatter, "cannot parse replay file: {error}"),
            ValidationError::TagReplayNotSupported => write!(formatter, "tag duel replays are not supported yet"),
            ValidationError::EmptyReplay => write!(formatter, "replay contains no responses"),
            ValidationError::Retry => write!(formatter, "engine rejected a response, the replay desynced"),
            ValidationError::TruncatedReplay => write!(formatter, "engine asked for a response but the replay ran out"),
            ValidationError::LeftoverResponses(count) => write!(formatter, "replay has {count} unconsumed responses after the duel ended"),
            ValidationError::ServerRejected => write!(formatter, "server rejected the session"),
            ValidationError::NoWin => write!(formatter, "duel ended without a win message"),
            ValidationError::DuelDidNotEnd => write!(formatter, "session ended before the duel finished"),
            ValidationError::Timeout => write!(formatter, "validation timed out"),
        }
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug)]
pub struct ValidationSummary {
    pub response_count: usize,
    pub winner: Option<Netplayer>,
}

enum Outcome {
    Continue,
    DuelEnded,
}

pub async fn validate_replay(path: &Path) -> Result<ValidationSummary, ValidationError> {
    ygopro::init();
    let bytes = std::fs::read(path).map_err(ValidationError::Io)?;
    let replay = Replay::read_le(&mut Cursor::new(bytes)).map_err(ValidationError::Parse)?;
    if replay.is_tag() {
        return Err(ValidationError::TagReplayNotSupported);
    }
    let response_count = replay.body.datas.len();
    if response_count == 0 {
        return Err(ValidationError::EmptyReplay);
    }
    REPLAY_SEED.set(replay.header.seed_sequence).ok();

    let (mut host, _duel_task) = SingleDuelHost::new(Default::default(), Configuration {
        allow_join_after_start: false,
        no_init_shuffle_deck: true,
        override_best_of: 0,
        seed_generator: None,
        replay_mode: ReplayMode::empty()
    });

    let (host_ctos_sender, host_ctos_receiver) = mpsc::unbounded_channel();
    let (client_ctos_sender, client_ctos_receiver) = mpsc::unbounded_channel();
    let mut host_stoc = host.add(UnboundedReceiverStream::new(host_ctos_receiver));
    let mut client_stoc = host.add(UnboundedReceiverStream::new(client_ctos_receiver));

    let mut host_info = replay.host_info();
    host_info.time_limit = 0;
    send(&host_ctos_sender, ctos::PlayerInfo {
        name: FixedLengthString::new(replay.body.host_name.to_string()),
    }.into());
    send(&host_ctos_sender, ctos::CreateGame {
        info: host_info,
        name: FixedLengthString::new(String::from("validate-replay")),
        pass: FixedLengthString::new(String::new()),
    }.into());
    send(&host_ctos_sender, ctos::JoinGame {
        version: ygopro::PRO_VERSION,
        gameid: 0,
        pass: FixedLengthString::new(String::new()),
    }.into());

    let mut responses: VecDeque<Vec<u8>> = replay.body.datas.into_iter().map(|data| data.data).collect();
    let mut saw_win = false;
    let mut winner = None;

    // Each `SingleDuelHost::add` client forwards its own messages through a
    // spawned task, so order across the two clients is not guaranteed. The
    // handshake therefore advances step by step, waiting for the server's
    // acknowledgement on the stoc stream before the next cross-client message.
    timeout(VALIDATION_TIMEOUT, async {
        wait_for(&mut host_stoc, |message| matches!(message, stoc::Message::TypeChange(_))).await?;

        send(&client_ctos_sender, ctos::PlayerInfo {
            name: FixedLengthString::new(replay.body.client_name.to_string()),
        }.into());
        send(&client_ctos_sender, ctos::JoinGame {
            version: ygopro::PRO_VERSION,
            gameid: 0,
            pass: FixedLengthString::new(String::new()),
        }.into());
        wait_for(&mut client_stoc, |message| matches!(message, stoc::Message::TypeChange(_))).await?;

        send(&host_ctos_sender, ctos::UpdateDeck {
            deck: build_deck(&replay.body.host_deck),
        }.into());
        send(&client_ctos_sender, ctos::UpdateDeck {
            deck: build_deck(&replay.body.client_deck),
        }.into());
        send(&host_ctos_sender, ctos::HsReady.into());
        send(&client_ctos_sender, ctos::HsReady.into());

        // `HsStart` silently returns unless both players are already ready.
        wait_for(&mut client_stoc, |message| matches!(message, stoc::Message::HsPlayerChange(status)
            if status.status.state() == PlayerChangeState::Ready && status.status.player() == Netplayer::Player(1))).await?;
        send(&host_ctos_sender, ctos::HsStart.into());
        wait_for(&mut host_stoc, |message| matches!(message, stoc::Message::SelectHand(_))).await?;

        // The replay does not record the hand result, so let the host win it
        // and go first. The deck order in the replay already encodes the first
        // attacker, so this choice does not affect the reproduction.
        send(&host_ctos_sender, ctos::HandResult {
            res: Hand::Rock,
        }.into());
        send(&client_ctos_sender, ctos::HandResult {
            res: Hand::Scissors,
        }.into());
        wait_for(&mut host_stoc, |message| matches!(message, stoc::Message::SelectTp(_))).await?;

        send(&host_ctos_sender, ctos::TpResult {
            result: CorePlayer::FirstAttackPlayer,
        }.into());

        loop {
            tokio::select! {
                message = host_stoc.next() => match message {
                    Some(message) => match handle_stoc(&message, &host_ctos_sender, &mut responses, &mut saw_win, &mut winner)? {
                        Outcome::Continue => (),
                        Outcome::DuelEnded => break,
                    },
                    None => return Err(ValidationError::DuelDidNotEnd),
                },
                message = client_stoc.next() => match message {
                    Some(message) => match handle_stoc(&message, &client_ctos_sender, &mut responses, &mut saw_win, &mut winner)? {
                        Outcome::Continue => (),
                        Outcome::DuelEnded => break,
                    },
                    None => return Err(ValidationError::DuelDidNotEnd),
                },
            }
        }
        if !saw_win {
            return Err(ValidationError::NoWin);
        }
        if !responses.is_empty() {
            return Err(ValidationError::LeftoverResponses(responses.len()));
        }
        Ok(())
    }).await.map_err(|_| ValidationError::Timeout)??;

    Ok(ValidationSummary { response_count, winner })
}

fn send(ctos_sender: &mpsc::UnboundedSender<ctos::Message>, message: ctos::Message) {
    ctos_sender.send(message).ok();
}

async fn wait_for<Predicate>(stream: &mut UnboundedReceiverStream<Complex<stoc::Message>>, predicate: Predicate) -> Result<(), ValidationError>
where Predicate: Fn(&stoc::Message) -> bool,
{
    while let Some(message) = stream.next().await {
        if predicate(message.deref()) {
            return Ok(());
        }
    }
    Err(ValidationError::DuelDidNotEnd)
}

fn build_deck(replay_deck: &ReplayDeck) -> Deck {
    // The replay stores both decks top-first; the wire format expects the main
    // and extra cards merged bottom-first. The `Deck::load` on the server side
    // re-splits the merged list, which restores the original separated order.
    let mut main = replay_deck.main.clone();
    main.reverse();
    let mut extra = replay_deck.extra.clone();
    extra.reverse();
    main.extend(extra);
    Deck {
        main,
        side: Vec::new(),
        extra: Vec::new(),
    }
}

fn handle_stoc(
    message: &Complex<stoc::Message>,
    ctos_sender: &mpsc::UnboundedSender<ctos::Message>,
    responses: &mut VecDeque<Vec<u8>>,
    saw_win: &mut bool,
    winner: &mut Option<Netplayer>,
) -> Result<Outcome, ValidationError> {
    match message.deref() {
        stoc::Message::GameMessage(game_message) => match &game_message.message {
            gm::Message::Win(win) => {
                *saw_win = true;
                *winner = match win.winner {
                    CorePlayer::FirstAttackPlayer => Some(Netplayer::Player(0)),
                    CorePlayer::SecondAttackPlayer => Some(Netplayer::Player(1)),
                    _ => None,
                };
            }
            gm::Message::Retry(_) => return Err(ValidationError::Retry),
            current_message if current_message.waiting_for().is_some() => {
                let response = responses.pop_front().ok_or(ValidationError::TruncatedReplay)?;
                ctos_sender.send(ctos::Message::Response(ctos::Response { response })).ok();
            }
            _ => (),
        },
        stoc::Message::DuelEnd(_) => return Ok(Outcome::DuelEnded),
        stoc::Message::ErrorMessage(_) => return Err(ValidationError::ServerRejected),
        _ => (),
    }
    Ok(Outcome::Continue)
}
