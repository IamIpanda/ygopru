//! Redirect the Lua `print` and `warn` into the Rust logger.
//!
//! Lua `print` goes to C buffer and often not show in time and escapes from the rust logger.
//! This plugin will override the Lua `print` and `warn` functions to redirect them into the Rust logger.
//! Only needed when you want to use `print` and `warn` in your Lua scripts.
//!
//! # Examples
//!
//! Enable the module:
//!
//! ```
//! let mut configuration = ygopro::Configuration::default();
//! configuration.enable_plugin("ygopro::plugin::print");
//! ```
//! 
//! In script:
//! ```lua
//! print("value", 42)
//! warn("something", "suspicious")
//! ```
//!

use std::cell::Cell;

use mlua::Lua;
use mlua::Value;
use ygopro_derive::before;
use ygopro_derive::register_to;

use crate::duel::Duel;
use crate::message as ygopro;
use crate::ygopro_handlers::HandlerEx;

/// Name for activitating this module in the plugin system.
pub static NAME: &'static str = module_path!();

#[before(ygopro::DuelInit)]
#[register_to(crate::ygopro_handlers::YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_duel_init(duel: &mut Duel) {
    let Some(lua) = (unsafe { duel.core.lua() }) else { return };
    install_warning(&lua);
    if let Err(error) = install_print(&lua) {
        log::warn!("failed to install the lua print: {error}");
    }
}

fn install_warning(lua: &Lua) {
    let pending = Cell::new(String::new());
    lua.set_warning_function(move |_, message, incomplete| {
        let mut buffer = pending.take();
        buffer.push_str(message);
        if incomplete {
            buffer.push('\t');
        } else {
            log::warn!("{buffer}");
            buffer.clear();
        }
        pending.set(buffer);
        Ok(())
    });
}

fn install_print(lua: &Lua) -> mlua::Result<()> {
    let print = lua.create_function(|lua, arguments: mlua::Variadic<Value>| {
        let tostring: mlua::Function = lua.globals().get("tostring")?;
        let mut line = String::new();
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 { line.push('\t'); }
            line.push_str(&tostring.call::<String>(argument.clone()).unwrap_or_default());
        }
        log::info!("{line}");
        Ok(())
    })?;
    lua.globals().set("print", print)?;
    Ok(())
}
