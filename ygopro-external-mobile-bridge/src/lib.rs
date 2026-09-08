//! Thin cdylib shell over `ygopro::ffi`.
//!
//! All server logic lives in the `ygopro` crate; this crate only keeps the
//! classic single-instance C ABI (`start_server` returning the bound port as
//! an `i32`) plus the Android JNI bindings, so existing callers see no change.

use std::ffi::CStr;
use std::ffi::c_char;
use std::sync::OnceLock;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;

use hashbrown::HashMap;
use parking_lot::Mutex;

use jni::JNIEnv;
use jni::objects::JClass;
use jni::objects::JString;
use jni::sys::jint;

use ygopro::cli::ArgsFormat;
use ygopro::cli::parse_cli_args;
use ygopro::ffi::ServerHandle;
use ygopro::ffi::init_core;
use ygopro::ffi::init_data;
use ygopro::ffi::init_lflist;
use ygopro::ffi::start_ygopro_server;
use ygopro::ffi::stop_ygopro_server;
use ygopro::ffi::ygopro_server_port;
use ygopro::managers::config_manager::ConfigManager;
use ygopro::managers::config_manager::set_global as set_config_manager;

const START_ALREADY_RUNNING: i32 = -1;
const START_BAD_ARGUMENTS: i32 = -2;
const START_RUNTIME_ERROR: i32 = -3;

static SERVER_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static SERVER_HANDLE: AtomicPtr<ServerHandle> = AtomicPtr::new(std::ptr::null_mut());

/// Start a single embedded server, returning the bound port or a negative
/// error code. Only one instance can run at a time; a second call while one
/// is running returns [`START_ALREADY_RUNNING`].
///
/// The resource base path is read from the arguments and the global card and
/// list data are (re)loaded from it before the server thread starts, mirroring
/// the standalone `ygoserver` behaviour.
///
/// # Safety
///
/// `arguments_pointer` must be null or point to a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start_server(arguments_pointer: *const c_char) -> i32 {
    let _guard = SERVER_LOCK.get_or_init(|| Mutex::new(())).lock();
    if !SERVER_HANDLE.load(Ordering::SeqCst).is_null() {
        return START_ALREADY_RUNNING;
    }

    let argument_text = if arguments_pointer.is_null() {
        String::new()
    } else {
        // Safety: guaranteed by the caller contract above.
        unsafe { CStr::from_ptr(arguments_pointer) }.to_string_lossy().to_string()
    };
    let arguments = split_command_line(&argument_text);

    // The embedded layout carries a base path that must seed the global data
    // before any server runs.
    let server_arguments = match parse_cli_args(&arguments, ArgsFormat::Mobile) {
        Ok(server_arguments) => server_arguments,
        Err(_) => return START_BAD_ARGUMENTS,
    };
    configure_resource_paths(&server_arguments);
    #[cfg(feature = "zip")]
    ygopro::ypk::archive_manager::init();
    init_data();
    init_lflist();
    init_core();

    let c_arguments = match std::ffi::CString::new(argument_text) {
        Ok(c_arguments) => c_arguments,
        Err(_) => return START_BAD_ARGUMENTS,
    };
    // Safety: `c_arguments` outlives the call and is NUL-terminated.
    let handle = unsafe { start_ygopro_server(c_arguments.as_ptr()) };
    if handle.is_null() {
        return START_RUNTIME_ERROR;
    }

    // Safety: `handle` came from `start_ygopro_server` and is not yet freed.
    let port = unsafe { ygopro_server_port(handle) };
    SERVER_HANDLE.store(handle, Ordering::SeqCst);
    port as i32
}

/// Stop the server started by [`start_server`], if any.
#[unsafe(no_mangle)]
pub extern "C" fn stop_server() {
    let _guard = SERVER_LOCK.get_or_init(|| Mutex::new(())).lock();
    let handle = SERVER_HANDLE.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !handle.is_null() {
        // Safety: ownership is transferred back to the callee, which frees it.
        unsafe { stop_ygopro_server(handle) };
    }
}

fn configure_resource_paths(server_arguments: &ygopro::cli::ServerArguments) {
    let mut entries: HashMap<String, String> = HashMap::new();
    entries.insert("path".to_string(), server_arguments.base_path.clone());
    let config = ConfigManager::from(entries);
    set_config_manager(config);
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

#[unsafe(no_mangle)]
pub extern "system" fn Java_cn_garymb_ygomobile_network_YGOServer_startServer(
    mut environment: JNIEnv,
    _class: JClass,
    arguments: JString,
) -> jint {
    let argument_text = match environment.get_string(&arguments) {
        Ok(argument_text) => argument_text.to_string_lossy().to_string(),
        Err(_) => return START_BAD_ARGUMENTS as jint,
    };
    let c_arguments = match std::ffi::CString::new(argument_text) {
        Ok(c_arguments) => c_arguments,
        Err(_) => return START_BAD_ARGUMENTS as jint,
    };
    // Safety: `c_arguments` is NUL-terminated and outlives the call.
    unsafe { start_server(c_arguments.as_ptr()) as jint }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_cn_garymb_ygomobile_network_YGOServer_stopServer(
    _environment: JNIEnv,
    _class: JClass,
) {
    stop_server();
}
