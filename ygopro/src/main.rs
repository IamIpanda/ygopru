// ygopro server main
// 对应: ../ygopro/gframe/gframe.cpp:28-151 (YGOPRO_SERVER_MODE)

use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_util::codec::LengthDelimitedCodec;

use ygopro_data::constants::Mode;
use ygopro_data::constants::MasterRule;
use ygopro_data::data::ReplayMode;
use ygopro_data::message::HostInfo;

#[tokio::main]
async fn main() {
    env_logger::init();
    let (port, _hostinfo, _replay_mode) = parse_args();
    start_server(port).await;
}

fn parse_args() -> (u16, HostInfo, ReplayMode) {
    let args = std::env::args().collect::<Vec<String>>();
    if args.len() > 1 && args.len() < 13 {
        log::error!("Bad param count. Please refer to readme, or don't use any param to quick test.");
        std::process::exit(1);
    } else if args.len() == 1 {
        return (0, HostInfo::default(), ReplayMode::empty());
    }

    let port: u16 = args[1].parse().unwrap_or(7911);

    let hostinfo = HostInfo {
        lflist: args[2].parse().unwrap_or(0),
        rule: args[3].parse().unwrap_or(0),
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

async fn start_server(port: u16) {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();
    println!("{port}");
    log::info!("listening on port {port}");

    loop {
        let (stream, _addr) = listener.accept().await.expect("Failed to accept connection");
        tokio::spawn(async move {
            let mut framed = LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .new_read(stream);

            while let Some(Ok(frame)) = framed.next().await {
                // frame: Bytes — raw ygopro ctos packet (flag + body)
            }
        });
    }
}
