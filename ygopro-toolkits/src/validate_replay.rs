use std::collections::VecDeque;
use std::io::Cursor;
use std::ops::Deref;
use std::path::Path;
use std::time::Duration;

use binrw::BinRead;
use futures::SinkExt;
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::codec::FramedRead;
use tokio_util::codec::FramedWrite;
use tokio_util::codec::LengthDelimitedCodec;

use ygopro::single_duel::Configuration;
use ygopro::single_duel::SingleDuelHost;
use ygopro_handler::RoomProvider;
use ygopro_data::complex::Complex;
use ygopro_data::constants::CorePlayer;
use ygopro_data::constants::Netplayer;
use ygopro_data::data::Replay;
use ygopro_data::data::Response;
use ygopro_data::message::ctos;
use ygopro_data::message::gm;
use ygopro_data::message::gm::GameMessage;
use ygopro_data::message::stoc;

use crate::start_game::start_game;
use crate::start_game::StartedDuel;

const START_GAME_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("cannot read replay file: {0}")]
    Io(std::io::Error),
    #[error("cannot parse replay file: {0}")]
    Parse(binrw::Error),
    #[error("tag duel replays are not supported yet")]
    TagReplayNotSupported,
    #[error("replay contains no responses")]
    EmptyReplay,
    #[error("engine rejected a response, the replay desynced")]
    Retry,
    #[error("replay has {0} unconsumed responses after the duel ended")]
    LeftoverResponses(usize),
    #[error("server rejected the session")]
    ServerRejected,
    #[error("duel ended without a win message")]
    NoWin,
    #[error("session ended before the duel finished")]
    DuelDidNotEnd,
    #[error("validation timed out")]
    Timeout,
}

#[derive(Debug)]
pub struct ValidationSummary {
    pub response_count: usize,
    pub winner: Option<Netplayer>,
    pub replayed_to_end: bool,
}

enum Outcome {
    Continue,
    DuelEnded,
    ReplayEnded,
}

struct DuelAbortGuard(tokio::task::JoinHandle<()>);

impl Drop for DuelAbortGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn bridge_observer(host: &mut SingleDuelHost, socket: TcpStream) {    let (viewer_reader, viewer_writer) = socket.into_split();
    let codec = LengthDelimitedCodec::builder()
        .length_field_length(2)
        .little_endian()
        .new_codec();
    let (client_to_server_sender, client_to_server_receiver) = mpsc::unbounded_channel();
    let client_to_server_codec = codec.clone();
    tokio::spawn(async move {
        let mut frames = FramedRead::new(viewer_reader, client_to_server_codec);
        while let Some(frame) = frames.next().await {
            if let Ok(frame) = frame {
                let mut cursor = Cursor::new(frame);
                if let Ok(message) = ctos::Message::read_le(&mut cursor) {
                    client_to_server_sender.send(message).ok();
                }
            }
        }
    });
    let mut server_to_client_stream = host.add(UnboundedReceiverStream::new(client_to_server_receiver));
    tokio::spawn(async move {
        let mut sink = FramedWrite::new(viewer_writer, codec);
        while let Some(message) = server_to_client_stream.next().await {
            let frame = message.data.clone();
            if sink.send(frame).await.is_err() {
                break;
            }
        }
    });
}

pub async fn validate_replay(path: &Path, wait_port: Option<u16>, timeout_seconds: u64) -> Result<ValidationSummary, ValidationError> {
    let validation_timeout = Duration::from_secs(timeout_seconds);
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

    let mut configuration = Configuration::default();
    configuration.no_init_shuffle_deck = true;
    configuration.no_mask = true;

    let started = timeout(START_GAME_TIMEOUT, start_game(&replay, configuration)).await
        .map_err(|_| ValidationError::Timeout)??;
    let StartedDuel {
        player1_ctos_sender,
        mut player1_stoc,
        player2_ctos_sender,
        mut player2_stoc,
        mut host,
        duel_task,
    } = started;
    let _duel_abort_guard = DuelAbortGuard(duel_task);

    let mut responses: VecDeque<Response> = replay.body.datas.into_iter().map(|data| data.data).collect();
    let mut saw_win = false;
    let mut replayed_to_end = false;
    let mut winner = None;

    if let Some(port) = wait_port {
        let listener = TcpListener::bind(("0.0.0.0", port))
            .await
            .expect("failed to bind the viewer port");
        log::info!("waiting for a viewer to join on port {port}");
        let (socket, viewer_addr) = listener.accept().await.expect("failed to accept the viewer");
        log::info!("viewer connected: {viewer_addr}");
        bridge_observer(&mut host, socket);
    }

    timeout(validation_timeout, async {
        loop {
            tokio::select! {
                message = player1_stoc.next() => match message {
                    Some(message) => {
                        log::debug!("player1 S← {:?}", message.deref());
                        match handle_stoc(&message, &player1_ctos_sender, &mut responses, &mut saw_win, &mut winner)? {
                            Outcome::Continue => (),
                            Outcome::DuelEnded => break,
                            Outcome::ReplayEnded => {
                                replayed_to_end = true;
                                break;
                            }
                        }
                    }
                    None => return Err(ValidationError::DuelDidNotEnd),
                },
                message = player2_stoc.next() => match message {
                    Some(message) => {
                        log::debug!("player2 S← {:?}", message.deref());
                        match handle_stoc(&message, &player2_ctos_sender, &mut responses, &mut saw_win, &mut winner)? {
                            Outcome::Continue => (),
                            Outcome::DuelEnded => break,
                            Outcome::ReplayEnded => {
                                replayed_to_end = true;
                                break;
                            }
                        }
                    }
                    None => return Err(ValidationError::DuelDidNotEnd),
                },
            }
        }
        if !saw_win && !replayed_to_end {
            return Err(ValidationError::NoWin);
        }
        if !responses.is_empty() {
            return Err(ValidationError::LeftoverResponses(responses.len()));
        }
        Ok(())
    }).await.map_err(|_| ValidationError::Timeout)??;

    Ok(ValidationSummary { response_count, winner, replayed_to_end })
}

fn handle_stoc(
    message: &Complex<stoc::Message>,
    ctos_sender: &mpsc::UnboundedSender<ctos::Message>,
    responses: &mut VecDeque<Response>,
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
                let Some(response) = responses.pop_front() else {
                    return Ok(Outcome::ReplayEnded);
                };
                log::debug!("C→ response {:?}", response);
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
