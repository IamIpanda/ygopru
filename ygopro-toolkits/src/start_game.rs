use std::ops::Deref;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use ygopro::single_duel::Configuration;
use ygopro::single_duel::SingleDuelHost;
use ygopro_core_wrapper::random::SEED_COUNT;
use ygopro_core_wrapper::DuelSeed;
use ygopro_data::complex::Complex;
use ygopro_data::constants::CorePlayer;
use ygopro_data::constants::Hand;
use ygopro_data::data::Deck;
use ygopro_data::data::Replay;
use ygopro_data::data::ReplayDeck;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::string::FixedLengthString;
use ygopro_handler::RoomProvider;

use crate::validate_replay::ValidationError;

static REPLAY_SEED: Mutex<Option<[u32; SEED_COUNT]>> = Mutex::new(None);

fn seed_generator(_duel_count: u8) -> DuelSeed {
    let guard = REPLAY_SEED.lock();
    let seed = guard.as_ref().copied().expect("replay seed is not initialized");
    DuelSeed::Complicated(seed)
}

pub struct StartedDuel {
    pub player1_ctos_sender: mpsc::UnboundedSender<ctos::Message>,
    pub player1_stoc: UnboundedReceiverStream<Complex<stoc::Message>>,
    pub player2_ctos_sender: mpsc::UnboundedSender<ctos::Message>,
    pub player2_stoc: UnboundedReceiverStream<Complex<stoc::Message>>,
    pub host: SingleDuelHost,
    pub duel_task: tokio::task::JoinHandle<()>,
}

pub async fn start_game(replay: &Replay, mut configuration: Configuration) -> Result<StartedDuel, ValidationError> {
    *REPLAY_SEED.lock() = Some(replay.header.seed_sequence);
    configuration.seed_generator = Some(seed_generator);

    let mut host_info = replay.host_info();
    host_info.time_limit = 0;

    let (mut host, duel_task) = SingleDuelHost::new(host_info, configuration);

    let (player1_ctos_sender, player1_ctos_receiver) = mpsc::unbounded_channel();
    let mut player1_stoc = host.add(UnboundedReceiverStream::new(player1_ctos_receiver));

    send(&player1_ctos_sender, ctos::PlayerInfo {
        name: FixedLengthString::new(replay.body.host_name.to_string()),
    }.into());
    send(&player1_ctos_sender, ctos::JoinGame {
        version: ygopro::PRO_VERSION,
        gameid: 0,
        pass: FixedLengthString::allocate()
    }.into());
    wait_for(&mut player1_stoc, stoc::MessageType::TypeChange).await?;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let (player2_ctos_sender, player2_ctos_receiver) = mpsc::unbounded_channel();
    let mut player2_stoc = host.add(UnboundedReceiverStream::new(player2_ctos_receiver));

    send(&player2_ctos_sender, ctos::PlayerInfo {
        name: FixedLengthString::new(replay.body.client_name.to_string()),
    }.into());
    send(&player2_ctos_sender, ctos::JoinGame {
        version: ygopro::PRO_VERSION,
        gameid: 0,
        pass: FixedLengthString::allocate()
    }.into());
    wait_for(&mut player2_stoc, stoc::MessageType::TypeChange).await?;

    wait_for(&mut player1_stoc, stoc::MessageType::HsPlayerEnter).await?;

    send(&player1_ctos_sender, ctos::UpdateDeck {
        deck: build_deck(&replay.body.host_deck),
    }.into());
    send(&player1_ctos_sender, ctos::HsReady.into());
    send(&player2_ctos_sender, ctos::UpdateDeck {
        deck: build_deck(&replay.body.client_deck),
    }.into());
    send(&player2_ctos_sender, ctos::HsReady.into());

    wait_for(&mut player2_stoc, stoc::MessageType::HsPlayerChange).await?;
    send(&player1_ctos_sender, ctos::HsStart.into());
    wait_for(&mut player1_stoc, stoc::MessageType::SelectHand).await?;

    send(&player1_ctos_sender, ctos::HandResult {
        res: Hand::Paper,
    }.into());
    send(&player2_ctos_sender, ctos::HandResult {
        res: Hand::Rock,
    }.into());
    wait_for(&mut player1_stoc, stoc::MessageType::SelectTp).await?;

    send(&player1_ctos_sender, ctos::TpResult {
        result: CorePlayer::FirstAttackPlayer,
    }.into());

    Ok(StartedDuel {
        player1_ctos_sender,
        player1_stoc,
        player2_ctos_sender,
        player2_stoc,
        host,
        duel_task,
    })
}

fn send(ctos_sender: &mpsc::UnboundedSender<ctos::Message>, message: ctos::Message) {
    log::debug!("C→ {:?}", ctos::MessageType::from(&message));
    ctos_sender.send(message).ok();
}

async fn wait_for(stream: &mut UnboundedReceiverStream<Complex<stoc::Message>>, message_type: stoc::MessageType) -> Result<(), ValidationError> {
    while let Some(message) = stream.next().await {
        let received_type = stoc::MessageType::from(message.deref());
        log::debug!("S← {:?}", received_type);
        if received_type == message_type {
            return Ok(());
        }
    }
    log::debug!("stream ended while waiting for {:?}", message_type);
    Err(ValidationError::DuelDidNotEnd)
}

fn build_deck(replay_deck: &ReplayDeck) -> Deck {
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
