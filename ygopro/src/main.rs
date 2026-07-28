use std::io::Cursor;

use binrw::BinRead;
use binrw::BinWrite;
use futures::SinkExt;
use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_util::codec::LengthDelimitedCodec;

use ygopro::single_duel::SingleDuelHost;
use ygopro_data::constants::Mode;
use ygopro_data::constants::MasterRule;
use ygopro_data::constants::Rule;
use ygopro_data::data::ReplayMode;
use ygopro_data::message::ctos;
use ygopro_data::message::HostInfo;
use ygopro_handler::RoomProvider;

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
    // TODO: decode base 64 seeds
    (port, hostinfo, replay_mode)
}

async fn start_server(port: u16, hostinfo: HostInfo, replay_mode: ReplayMode) {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();
    println!("{port}");
    log::info!("listening on port {port}");
    let (mut duel, handle) = SingleDuelHost::new(hostinfo.mode == Mode::Match, None);

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
                        log::debug!("CTOS: {message:?}");
                    })
                }
                Err(_) => None,
            });

            let mut stoc_stream = duel.add(ctos_stream);

            tokio::spawn(async move {
                while let Some(message) = stoc_stream.next().await {
                    log::debug!("STOC: {message:?}");
                    let mut buffer = Cursor::new(Vec::new());
                    if message.write_le(&mut buffer).is_ok() {
                        framed_write.send(buffer.into_inner().into()).await.ok();
                    }
                }
            });
        }
    });

    handle.await.ok();
}
