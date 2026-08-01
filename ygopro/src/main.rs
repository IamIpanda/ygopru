use std::io::Cursor;
use std::ops::Deref;
use std::sync::OnceLock;

use base64::Engine;
use binrw::BinRead;
use futures::SinkExt;
use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_util::codec::LengthDelimitedCodec;

use ygopro::single_duel::SingleDuelHost;
use ygopro_core_wrapper::DuelSeed;
use ygopro_data::constants::Mode;
use ygopro_data::constants::MasterRule;
use ygopro_data::constants::Rule;
use ygopro_data::data::ReplayMode;
use ygopro_data::message::ctos;
use ygopro_data::message::HostInfo;
use ygopro_handler::RoomProvider;

/// Seeds decoded from command line args[13..], indexed by duel count.
/// Mirrors pre_seed[duel_count] in ../ygopro/gframe/single_duel.cpp:548.
static PRE_SEEDS: OnceLock<Vec<[u32; ygopro_core_wrapper::random::SEED_COUNT]>> = OnceLock::new();

/// Decode one base64 seed blob into the seed sequence, mirrors Base64::Decode in ../ygopro/gframe/gframe.cpp:112.
fn decode_seed(seed_arg: &str) -> [u32; ygopro_core_wrapper::random::SEED_COUNT] {
    let bytes = base64::engine::general_purpose::STANDARD.decode(seed_arg).unwrap();
    let mut seed = [0u32; ygopro_core_wrapper::random::SEED_COUNT];
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        seed[index] = u32::from_le_bytes(chunk.try_into().unwrap());
    }
    seed
}

/// Provide the pre-seeded sequence for the given duel, or random if not specified.
fn seed_generator(duel_count: u8) -> DuelSeed {
    match PRE_SEEDS.get().and_then(|seeds| seeds.get(duel_count as usize)).copied() {
        Some(seed) => DuelSeed::Complicated(seed),
        None => DuelSeed::None,
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    ygopro::init();
    let (port, hostinfo, replay_mode) = parse_args();
    start_server(port, hostinfo, replay_mode).await;
}

fn parse_args() -> (u16, HostInfo, ReplayMode) {
    let args = std::env::args().collect::<Vec<String>>();
    if args.len() > 2 && args.len() < 13 {
        log::error!("Bad param count. Please refer to readme, or don't use any param to quick test.");
        std::process::exit(1);
    } else if args.len() == 1 {
        return (0, HostInfo::default(), ReplayMode::empty());
    } else if args.len() == 2 {
        let port: u16 = args[1].parse().expect("Cannot parse port number");
        return (port, HostInfo::default(), ReplayMode::empty());
    }

    let port: u16 = args[1].parse().expect("Cannot parse port number");

    let hostinfo = HostInfo {
        lflist: args[2].parse().unwrap_or(0),
        rule: Rule::from_bits_retain(args[3].parse::<u8>().unwrap_or(0)),
        mode: match args[4].parse::<u8>().unwrap_or(0) {
            m if m > 2 => Mode::Single,
            m => Mode::try_from(m).unwrap_or(Mode::Single),
        },
        duel_rule: if args[5] == "T" {
            MasterRule::MasterRuleNew
        } else if args[5] == "F" {
            MasterRule::MasterRule2020
        } else if let Ok(r) = args[5].parse::<u8>() {
            if r != 0 { MasterRule::try_from(r).unwrap_or(MasterRule::MasterRule2020) }
            else { MasterRule::MasterRule2020 }
        } else {
            MasterRule::MasterRule2020
        },
        no_check_deck: args[6] == "T",
        no_shuffle_deck: args[7] == "T",
        start_lp: args[8].parse().unwrap_or(8000),
        start_hand: args[9].parse().unwrap_or(5),
        draw_count: args[10].parse().unwrap_or(1),
        time_limit: args[11].parse().unwrap_or(180),
    };
    let replay_mode = ReplayMode::from_bits_retain(args[12].parse::<u32>().unwrap_or(0));
    let pre_seeds = args.iter().skip(13).map(|seed_arg| decode_seed(seed_arg)).collect();
    PRE_SEEDS.set(pre_seeds).ok();
    (port, hostinfo, replay_mode)
}

async fn start_server(port: u16, hostinfo: HostInfo, replay_mode: ReplayMode) {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();
    println!("{port}");
    log::info!("listening on port {port}");
    let configuration = ygopro::single_duel::Configuration { 
        no_init_shuffle_deck: false, 
        allow_join_after_start: true, 
        seed_generator: Some(seed_generator), 
        override_best_of: 0,
        replay_mode
    };
    let (mut duel, handle) = SingleDuelHost::new(hostinfo, configuration);

    tokio::spawn(async move {
        loop {
            let (stream, _addr) = listener.accept().await.expect("Failed to accept connection");
            let (reader, writer) = stream.into_split();
            let framed_read = LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .little_endian()
                .new_read(reader);
            let mut framed_write = LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .little_endian()
                .new_write(writer);

            let ctos_stream = framed_read.filter_map(|result| match result {
                Ok(frame) => {
                    let mut cursor = Cursor::new(&frame);
                    ctos::Message::read_le(&mut cursor).ok().inspect(|message| {
                        log::trace!("CTOS: {message:?}");
                    })
                }
                Err(_) => None,
            });

            let mut stoc_stream = duel.add(ctos_stream);

            tokio::spawn(async move {
                while let Some(message) = stoc_stream.next().await {
                    log::trace!("STOC: {:?}", message.deref());
                    framed_write.send(message.data).await.ok();
                }
            });
        }
    });

    handle.await.ok();
}
