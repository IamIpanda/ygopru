//! Toolkits for starting ygopro as a local server.
//!
//! # Examples
//!
//! Just like what `main` do:
//!
//! ```rust,no_run
//! use ygopro::cli::*;
//!
//! # async fn run() {
//! let args = std::env::args().skip(1).collect::<Vec<String>>();
//! let server_arguments = parse_cli_args(&args, Default::default()).unwrap();
//! let duel = build_duel_host(server_arguments.host_info, server_arguments.replay_mode, server_arguments.seeds);
//! start_local_server(server_arguments.port, duel).await;
//! # }
//! ```

use std::io::Cursor;
use std::path::Path;

use binrw::BinRead;
use futures::SinkExt;
use tokio::net::TcpListener;

use base64::Engine;
use tokio_stream::StreamExt;
use tokio_util::codec::LengthDelimitedCodec;
use ygopro_core_wrapper::DuelSeed;
use ygopro_core_wrapper::random::SEED_COUNT;
use ygopro_data::complex::Complex;
use ygopro_data::constants::*;
use ygopro_data::message::{HostInfo, ctos, stoc};
use ygopro_data::data::ReplayMode;
use ygopro_handler::RoomProvider;

use crate::Configuration;
use crate::host::DuelHost;

/// Error type for command line argument parsing.
#[derive(Debug)]
pub enum CliError {
    /// Params is not enough or too many.
    BadParamCount,
    // Cannot parse port number.
    InvalidPort(String),
    // Cannot decode seed.
    InvalidSeed(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::BadParamCount => write!(f, "Bad param count. Please refer to readme, or don't use any param to quick test."),
            CliError::InvalidPort(port) => write!(f, "Cannot parse port number: {port}"),
            CliError::InvalidSeed(seed) => write!(f, "Cannot decode seed: {seed}"),
        }
    }
}

impl std::error::Error for CliError {}

/// Parsed arguments for starting a local duel server.
pub struct ServerArguments {
    pub port: u16,
    pub host_info: HostInfo,
    pub replay_mode: ReplayMode,
    pub seeds: Vec<[u32; SEED_COUNT]>,
    pub base_path: String,
}

/// Argument layout of the calling client.
///
/// Both layouts use the same leading fields (port lflist rule mode duel_rule
/// ... replay_mode); they differ only in what follows. The caller is expected
/// to strip any program name before parsing.
#[derive(Default)]
pub enum ArgsFormat {
    /// No base path: seeds start right after replay_mode.
    #[default]
    Mycard,
    /// A base path sits after replay_mode, followed by two reserved fields,
    /// then seeds.
    Mobile,
    /// Decide by inspecting the field right after replay_mode: a path-looking
    /// value means [`ArgsFormat::Mobile`], a base64-decodable one means
    /// [`ArgsFormat::Mycard`].
    Unknown,
}

/// Parsed arguments for starting a local duel server [`ServerArguments`].
///
/// # Examples
///
/// ```
/// use ygopro::cli::parse_cli_args;
///
/// let args = vec!["2334".to_string()];
/// let server_arguments = parse_cli_args(&args, Default::default()).unwrap();
/// assert_eq!(server_arguments.port, 2334);
/// ```
pub fn parse_cli_args(args: &[String], format: ArgsFormat) -> Result<ServerArguments, CliError> {
    let args = match args.len() {
        0 => return Ok(ServerArguments {
            port: 0,
            host_info: HostInfo::default(),
            replay_mode: ReplayMode::empty(),
            seeds: Vec::new(),
            base_path: String::from("./"),
        }),
        1 => {
            let port: u16 = args[0].parse().map_err(|_| CliError::InvalidPort(args[0].clone()))?;
            return Ok(ServerArguments {
                port,
                host_info: HostInfo::default(),
                replay_mode: ReplayMode::empty(),
                seeds: Vec::new(),
                base_path: String::from("./"),
            });
        }
        length if length < 12 => return Err(CliError::BadParamCount),
        _ => args,
    };

    let format = match format {
        ArgsFormat::Mycard | ArgsFormat::Mobile => format,
        ArgsFormat::Unknown => match args.get(12) {
            Some(tail) => infer_format(tail)?,
            None => ArgsFormat::Mycard,
        },
    };

    let port: u16 = args[0].parse().map_err(|_| CliError::InvalidPort(args[0].clone()))?;
    let deck_manager = crate::managers::deck_manager::load();
    let host_info = HostInfo {
        lflist: deck_manager.get_lflist_by_index(args[1].parse().unwrap_or(0)).map(|l| l.hash).unwrap_or(0),
        rule: Rule::try_from(args[2].parse::<u8>().unwrap_or(0)).unwrap_or(Rule::All),
        mode: parse_mode(&args[3]),
        duel_rule: parse_duel_rule(&args[4]),
        no_check_deck: args[5] == "T",
        no_shuffle_deck: args[6] == "T",
        start_lp: args[7].parse().unwrap_or(8000),
        start_hand: args[8].parse().unwrap_or(5),
        draw_count: args[9].parse().unwrap_or(1),
        time_limit: args[10].parse().unwrap_or(180),
    };
    let replay_mode = ReplayMode::from_bits_retain(args[11].parse::<u32>().unwrap_or(0));

    match format {
        ArgsFormat::Mobile => {
            let base_path = args.get(12).cloned().unwrap_or_else(|| String::from("./"));
            let seeds = args.iter().skip(13).map(|seed_argument| decode_seed(seed_argument)).collect::<Result<Vec<_>, _>>()?;
            Ok(ServerArguments {
                port,
                host_info,
                replay_mode,
                seeds,
                base_path,
            })
        }
        ArgsFormat::Mycard => {
            let seeds = args.iter().skip(12).map(|seed_argument| decode_seed(seed_argument)).collect::<Result<Vec<_>, _>>()?;
            Ok(ServerArguments {
                port,
                host_info,
                replay_mode,
                seeds,
                base_path: String::from("./"),
            })
        }
        ArgsFormat::Unknown => unreachable!(),
    }
}

fn infer_format(tail: &str) -> Result<ArgsFormat, CliError> {
    if Path::new(tail).exists() {
        return Ok(ArgsFormat::Mobile);
    }
    match base64::engine::general_purpose::STANDARD.decode(tail) {
        Ok(_) => Ok(ArgsFormat::Mycard),
        Err(_) => Err(CliError::BadParamCount),
    }
}

fn decode_seed(seed_arg: &str) -> Result<[u32; SEED_COUNT], CliError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(seed_arg).map_err(|_| CliError::InvalidSeed(seed_arg.to_string()))?;
    let mut seed = [0u32; ygopro_core_wrapper::random::SEED_COUNT];
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        if index >= seed.len() {
            break;
        }
        seed[index] = u32::from_le_bytes(chunk.try_into().unwrap());
    }
    Ok(seed)
}

fn parse_mode(argument: &str) -> Mode {
    match argument.parse::<u8>().unwrap_or(0) {
        mode_value if mode_value > 2 => Mode::Single,
        mode_value => Mode::try_from(mode_value).unwrap_or(Mode::Single),
    }
}

fn parse_duel_rule(argument: &str) -> MasterRule {
    if argument == "T" {
        MasterRule::MasterRuleNew
    } else if argument == "F" {
        MasterRule::MasterRule2020
    } else if let Ok(rule_value) = argument.parse::<u8>() {
        if rule_value != 0 {
            MasterRule::try_from(rule_value).unwrap_or(MasterRule::MasterRule2020)
        } else {
            MasterRule::MasterRule2020
        }
    } else {
        MasterRule::MasterRule2020
    }
}

/// Build a [`DuelHost`] from host info, replay mode, and pre-seeds.
///
/// # Examples
///
/// ```rust,no_run
/// use ygopro::cli::build_duel_host;
/// use ygopro_data::message::HostInfo;
/// use ygopro_data::data::ReplayMode;
///
/// let duel = build_duel_host(HostInfo::default(), ReplayMode::empty(), Vec::new());
/// ```
pub fn build_duel_host(hostinfo: HostInfo, replay_mode: ReplayMode, pre_seeds: Vec<[u32; SEED_COUNT]>) -> DuelHost {
    let mut configuration = Configuration::default();
    configuration.seed_generator = Some(Box::new(move |duel_count: u8| {
        match pre_seeds.get(duel_count as usize).copied() {
            Some(seed) => DuelSeed::Complicated(seed),
            None => DuelSeed::None,
        }
    }));
    configuration.enable_plugin_with_configuration(crate::plugin::replay::NAME, crate::plugin::replay::Configuration { mode: replay_mode });
    DuelHost::new(hostinfo, configuration)
}

/// Start a one-shot local server on a fixed port with an built [`DuelHost`].
///
/// # Examples
/// 
/// ```rust,no_run
/// use ygopro::cli::{build_duel_host, start_local_server};
/// use ygopro_data::message::HostInfo;
/// use ygopro_data::data::ReplayMode;
///
/// # async fn run() {
/// let duel = build_duel_host(HostInfo::default(), ReplayMode::empty(), Vec::new());
/// start_local_server(0, duel).await;
/// # }
/// ```
pub async fn start_local_server(port: u16, duel: impl RoomProvider<ctos::Message, Complex<stoc::Message>> + Send + 'static) {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.expect("Failed to bind to port");
    let port = listener.local_addr().expect("Failed to get random port").port();
    println!("{port}");
    log::info!("listening on port {port}");
    start_local_server_with_listener(listener, duel).await;
}

/// Start a one-shot local server with an existing [`TcpListener`].
pub async fn start_local_server_with_listener(
    listener: TcpListener,
    mut duel: impl RoomProvider<ctos::Message, Complex<stoc::Message>> + Send + 'static,
) {
    let finish_signal = duel.get_finish_signal();

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
                    ctos::Message::read_le(&mut cursor).ok()
                }
                Err(_) => None,
            });

            let mut stoc_stream = duel.add(ctos_stream);

            tokio::spawn(async move {
                while let Some(message) = stoc_stream.next().await {
                    framed_write.send(message.data).await.ok();
                }
            });
        }
    });

    finish_signal.await;
}
