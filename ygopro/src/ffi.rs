//! C ABI entry points for embedding one or more duel servers.
//!
//! Every call hands back an opaque [`ServerHandle`] that the caller owns, so
//! multiple independent servers can run in one process at once.
//!
//! The caller must run one of the `init_*` exports before the first
//! [`start_ygopro_server`]. The stepwise exports allow customizing which
//! global managers are seeded; [`init`] runs the full default sequence.

use std::ffi::CStr;
use std::ffi::c_char;
use std::io;
use std::thread::JoinHandle;
use std::time::Duration;

use hashbrown::HashMap;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::cli;
use crate::managers;

/// Opaque handle to a running embedded server.
pub struct ServerHandle {
    shutdown_sender: oneshot::Sender<()>,
    server_thread: JoinHandle<()>,
    port: u16,
}

/// Run the full default initialization sequence before the first
/// [`start_ygopro_server`].
#[unsafe(no_mangle)]
pub extern "C" fn init() {
    init_config();
    init_i18n();
    #[cfg(feature = "zip")]
    init_expansion();
    init_data();
    init_lflist();
    init_core();
}

/// Seed the global configuration manager from `system.conf` and the
/// `YGOPRO_` environment variables.
#[unsafe(no_mangle)]
pub extern "C" fn init_config() {
    managers::config_manager::init();
}

/// Seed the global string table from `strings.conf`.
#[unsafe(no_mangle)]
pub extern "C" fn init_i18n() {
    managers::i18n::init();
}

/// Scan the expansion archives.
#[cfg(feature = "zip")]
#[unsafe(no_mangle)]
pub extern "C" fn init_expansion() {
    crate::ypk::archive_manager::init();
}

/// Load the card database.
#[unsafe(no_mangle)]
pub extern "C" fn init_data() {
    managers::data_manager::init();
}

/// Load the forbidden/limited lists.
#[unsafe(no_mangle)]
pub extern "C" fn init_lflist() {
    managers::deck_manager::init();
}

/// Register the core callbacks (script/card readers, message handler).
#[unsafe(no_mangle)]
pub extern "C" fn init_core() {
    crate::init_core();
}

/// Start an embedded server from a C string of whitespace-separated arguments,
/// returning an owned [`ServerHandle`] or null on failure.
///
/// The caller must have run one of the `init_*` exports beforehand; this
/// function performs no initialization of its own.
///
/// # Safety
///
/// `arguments_pointer` must be null or point to a valid NUL-terminated string.
/// The returned handle must later be passed to [`stop_ygopro_server`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start_ygopro_server(arguments_pointer: *const c_char) -> *mut ServerHandle {
    let argument_text = if arguments_pointer.is_null() {
        String::new()
    } else {
        // Safety: guaranteed by the caller contract above.
        unsafe { CStr::from_ptr(arguments_pointer) }.to_string_lossy().to_string()
    };
    let arguments = split_command_line(&argument_text);

    let server_arguments = match cli::parse_cli_args(&arguments, cli::ArgsFormat::Unknown) {
        Ok(server_arguments) => server_arguments,
        Err(error) => {
            log::error!("Failed to parse server arguments: {error}");
            return std::ptr::null_mut();
        }
    };

    configure_resource_paths(&server_arguments);

    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let (start_result_sender, start_result_receiver) = std::sync::mpsc::channel();
    let server_thread = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                log::error!("Failed to create Tokio runtime: {error}");
                start_result_sender.send(Err(START_RUNTIME_ERROR)).ok();
                return;
            }
        };

        runtime.block_on(async move {
            if let Err(error) =
                run_tcp_server(server_arguments, shutdown_receiver, start_result_sender).await
            {
                log::error!("TCP server stopped with error: {error}");
            }
        });
        runtime.shutdown_timeout(Duration::from_secs(2));
    });

    let port = match start_result_receiver.recv() {
        Ok(Ok(port)) => port,
        Ok(Err(error_code)) => {
            log::error!("Server failed to start with code {error_code}");
            server_thread.join().ok();
            return std::ptr::null_mut();
        }
        Err(_) => {
            server_thread.join().ok();
            return std::ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(ServerHandle {
        shutdown_sender,
        server_thread,
        port,
    }))
}

/// Return the port the server bound to, or 0 for a null handle.
///
/// # Safety
///
/// `handle` must be null or a pointer previously returned by
/// [`start_ygopro_server`] that has not yet been passed to
/// [`stop_ygopro_server`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ygopro_server_port(handle: *mut ServerHandle) -> u16 {
    if handle.is_null() {
        return 0;
    }
    // Safety: guaranteed by the caller contract above.
    unsafe { &*handle }.port
}

/// Stop the server and free its handle. Null handles are ignored.
///
/// # Safety
///
/// `handle` must be null or a pointer previously returned by
/// [`start_ygopro_server`] that has not yet been passed to
/// [`stop_ygopro_server`]. The handle is freed here and must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stop_ygopro_server(handle: *mut ServerHandle) {
    if handle.is_null() {
        return;
    }
    // Safety: guaranteed by the caller contract above; ownership is returned.
    let handle = unsafe { Box::from_raw(handle) };
    handle.shutdown_sender.send(()).ok();
    handle.server_thread.join().ok();
}

const START_RUNTIME_ERROR: i32 = -3;
const START_BIND_ERROR: i32 = -4;

async fn run_tcp_server(
    server_arguments: cli::ServerArguments,
    shutdown_receiver: oneshot::Receiver<()>,
    start_result_sender: std::sync::mpsc::Sender<Result<u16, i32>>,
) -> io::Result<()> {
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", server_arguments.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            start_result_sender.send(Err(START_BIND_ERROR)).ok();
            return Err(error);
        }
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => {
            start_result_sender.send(Err(START_BIND_ERROR)).ok();
            return Err(error);
        }
    };
    start_result_sender.send(Ok(port)).ok();
    log::info!("Listening on port {port}");

    let duel = cli::build_duel_host(
        server_arguments.host_info,
        server_arguments.replay_mode,
        server_arguments.seeds,
    );
    tokio::select! {
        _ = shutdown_receiver => {}
        _ = cli::start_local_server_with_listener(listener, duel) => {}
    }
    Ok(())
}

fn configure_resource_paths(server_arguments: &cli::ServerArguments) {
    let mut entries: HashMap<String, String> = HashMap::new();
    entries.insert("path".to_string(), server_arguments.base_path.clone());
    let config = managers::config_manager::ConfigManager::from(entries);
    managers::config_manager::set_global(config);
}

fn split_command_line(arguments: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_argument = String::new();
    let mut in_quotes = false;

    for character in arguments.chars() {
        if character == '"' {
            in_quotes = !in_quotes;
        } else if character.is_whitespace() && !in_quotes {
            if !current_argument.is_empty() {
                result.push(std::mem::take(&mut current_argument));
            }
        } else {
            current_argument.push(character);
        }
    }

    if !current_argument.is_empty() {
        result.push(current_argument);
    }

    result
}
