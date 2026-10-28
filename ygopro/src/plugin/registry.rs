//! A string registry visible to lua scripts, shared by the duels of a match.
//!
//! The values outlive a single duel: what one duel writes, the next duel of the same match reads
//! back, while another match sees nothing. Scripts reach it through the `Duel` table:
//!
//! - `Duel.GetRegistryValue(key)` returns the stored string, or `nil` when the key is absent.
//! - `Duel.SetRegistryValue(key, value)` stores a value, removing the key when `value` is `nil`.
//! - `Duel.GetRegistryKeys()` returns every key as a separate result, or nothing when empty.
//! - `Duel.GetRegistry()` returns a table holding every pair.
//! - `Duel.SetRegistry(table, override)` stores every pair, clearing the registry first when
//!   `override` is true.
//! - `Duel.ClearRegistry()` removes everything.
//!
//! Keys and values go through `tostring`.
//!
//! ```
//! let mut configuration = ygopro::Configuration::default();
//! configuration.enable_plugin("ygopro::plugin::registry");
//! ```
//!
//! An effect that may be activated only once per match:
//!
//! ```lua
//! function s.cost(e,tp,eg,ep,ev,re,r,rp,chk)
//! 	if chk==0 then return Duel.GetRegistryValue(id)==nil end
//! 	Duel.SetRegistryValue(id,"1")
//! end
//! ```

use std::collections::HashMap;

use linkme::distributed_slice;

use ygopro_derive::Attachment;
use ygopro_derive::before;
use ygopro_derive::register_to;

use crate::duel::Duel;
use crate::message as ygopro;
use crate::ygopro_handlers::HandlerEx;

/// Name for activitating this module in the plugin system.
pub static NAME: &'static str = module_path!();

#[derive(Attachment)]
pub struct Attachment {
    entries: HashMap<String, String>
}

#[before(ygopro::DuelInit)]
#[register_to(crate::ygopro_handlers::YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_duel_init(duel: &mut Duel, attachment: &mut Attachment) {
    if let Some(lua) = unsafe { duel.core.lua() } && let Err(error) = install(&lua, &attachment.entries) {
        log::warn!("failed to install the lua registry: {error}");
    }
}

#[before(ygopro::DuelEnd)]
#[register_to(crate::ygopro_handlers::YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_before_duel_end(duel: &mut Duel, attachment: &mut Attachment) {
    if let Some(lua) = unsafe { duel.core.lua() } && let Some(entries) = dump(&lua) {
        attachment.entries = entries;
    }
}

fn install(lua: &mlua::Lua, entries: &HashMap<String, String>) -> mlua::Result<()> {
    let store = lua.create_table()?;
    for (key, value) in entries {
        store.raw_set(key.as_str(), value.as_str())?;
    }
    lua.set_named_registry_value(NAME, store)?;

    let duel: mlua::Table = lua.globals().get("Duel")?;

    duel.set("GetRegistryValue", lua.create_function(|lua, key: mlua::Value| match coerce_string(lua, key)? {
        Some(key) => storage(lua)?.raw_get::<Option<String>>(key),
        None => Ok(None)
    })?)?;

    duel.set("SetRegistryValue", lua.create_function(|lua, (key, value): (mlua::Value, mlua::Value)| {
        let key = coerce_string(lua, key)?.ok_or_else(|| mlua::Error::runtime("invalid key for registry"))?;
        let store = storage(lua)?;
        match value {
            mlua::Value::Nil => store.raw_set(key, mlua::Value::Nil),
            value => {
                if let Some(value) = coerce_string(lua, value)? {
                    store.raw_set(key, value)?;
                }
                Ok(())
            }
        }
    })?)?;

    duel.set("GetRegistryKeys", lua.create_function(|lua, ()| {
        let mut results = mlua::MultiValue::new();
        for pair in storage(lua)?.pairs::<mlua::Value, mlua::Value>() {
            let (key, _) = pair?;
            results.push_back(key);
        }
        if results.is_empty() {
            results.push_back(mlua::Value::Nil);
        }
        Ok(results)
    })?)?;

    duel.set("GetRegistry", lua.create_function(|lua, ()| {
        let table = lua.create_table()?;
        for pair in storage(lua)?.pairs::<mlua::Value, mlua::Value>() {
            let (key, value) = pair?;
            table.raw_set(key, value)?;
        }
        Ok(table)
    })?)?;

    duel.set("SetRegistry", lua.create_function(|lua, (table, override_registry): (mlua::Table, Option<mlua::Value>)| {
        let store = storage(lua)?;
        if matches!(override_registry, Some(mlua::Value::Boolean(true))) {
            store.clear()?;
        }
        for pair in table.pairs::<mlua::Value, mlua::Value>() {
            let (key, value) = pair?;
            if let (Some(key), Some(value)) = (coerce_string(lua, key)?, coerce_string(lua, value)?) {
                store.raw_set(key, value)?;
            }
        }
        Ok(())
    })?)?;

    duel.set("ClearRegistry", lua.create_function(|lua, ()| storage(lua)?.clear())?)?;

    Ok(())
}

fn storage(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
    lua.named_registry_value(NAME)
}

fn dump(lua: &mlua::Lua) -> Option<HashMap<String, String>> {
    let store = storage(lua).ok()?;
    Some(store.pairs::<String, String>().flatten().collect())
}

fn coerce_string(lua: &mlua::Lua, value: mlua::Value) -> mlua::Result<Option<String>> {
    Ok(lua.coerce_string(value)?.and_then(|value| value.to_str().ok()).map(|value| value.to_string()))
}

