//! Register Rust implementations in the core's existing Lua environment.
//!
//! These low-level bindings use the Lua build selected for the core; no second
//! Lua runtime is linked. See [`Duel::register_lua_function`] for callback safety.
//!
//! # 使用示例
//!
//! 在 Rust 中实现回调，创建决斗后、加载卡片脚本或开始决斗前注册：
//!
//! ```no_run
//! use ygopro_core_wrapper::{Duel, DuelSeed, lua};
//!
//! unsafe extern "C" fn get_lp(state: *mut lua::State) -> std::ffi::c_int {
//!	// 在这里实现替换逻辑；示例固定返回 12345。
//!	unsafe { lua::push_integer(state, 12345) };
//!	1 // 返回压入 Lua 栈的结果数量。
//! }
//!
//! let mut duel = Duel::new(DuelSeed::Single(42));
//! unsafe {
//!	duel.register_lua_function("Duel", "GetLP", get_lp)
//!		.expect("替换 Duel.GetLP 失败");
//! }
//! ```
//!
//! 此后，这场决斗中的 Lua 调用 `Duel.GetLP(player)` 会执行上述 Rust 函数。
//! 示例只改变 API 返回值，不修改内核实际 LP。每场新决斗都需要重新注册。
//! 回调不能跨 FFI panic，也不能跨 Rust 栈触发 Lua error 或 yield。

use std::ffi::{CString, c_char, c_int};

use crate::{Duel, intptr_t};

/// Opaque Lua state, owned by the core. Never free or retain this pointer.
#[repr(C)]
pub struct State {
	_private: [u8; 0],
}

/// A Lua C function: read arguments, push results, return the result count.
pub type Function = unsafe extern "C" fn(*mut State) -> c_int;

/// Failure to register a Rust implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationError {
	/// The duel has already ended.
	DuelEnded,
	/// A name contains a NUL byte, or the function name is empty.
	InvalidName,
	/// The named global is absent or is not a table.
	MissingTable,
	/// Lua could not complete registration (for example, allocation failure).
	LuaError,
}

unsafe extern "C" {
	fn ygopru_register_lua_function(pointer: intptr_t, table: *const c_char,
		name: *const c_char, callback: Function) -> c_int;

	/// Return the number of stack entries, initially the argument count.
	///
	/// # Safety
	/// `state` must be the live state passed to the current callback, on its thread.
	#[link_name = "ygopru_lua_get_top"]
	pub fn get_top(state: *mut State) -> c_int;

	/// Read an integer; writes 1 to `valid` on conversion success, otherwise 0.
	///
	/// # Safety
	/// Requires the current callback's live state, a valid Lua stack index, and
	/// a writable `valid` pointer (or null to ignore conversion success).
	#[link_name = "ygopru_lua_to_integer"]
	pub fn to_integer(state: *mut State, index: c_int, valid: *mut c_int) -> i64;

	/// Push an integer result.
	///
	/// # Safety
	/// Requires the current callback's live state and a free stack slot.
	/// Lua guarantees at least 20 free slots at callback entry.
	/// The value must fit the selected core's `lua_Integer`.
	#[link_name = "ygopru_lua_push_integer"]
	pub fn push_integer(state: *mut State, value: i64);

	/// Push a boolean result (zero is false, nonzero is true).
	///
	/// # Safety
	/// Requires the current callback's live state and a free stack slot.
	#[link_name = "ygopru_lua_push_boolean"]
	pub fn push_boolean(state: *mut State, value: c_int);
}

impl Duel {
	/// Add or replace `table[name]` with a Rust callback for this duel only.
	/// Use an empty `table` for a global function, e.g. `print`. Table names are
	/// literal global names such as `Duel` or `Card`, not dotted paths.
	///
	/// Register after creation and before loading card scripts / starting play.
	/// Existing Lua references to the old function are not changed. Core startup
	/// scripts have already run when `Duel::new` returns.
	///
	/// # Safety
	/// The callback must obey Lua's stack and result-count rules on every call.
	/// It must not unwind/panic across FFI, raise a Lua error, or yield across
	/// Rust frames. Do not call Lua APIs that can longjmp or throw through Rust.
	/// Do not retain the state pointer, access it concurrently, or reenter/end
	/// the duel from a callback. Callback code must remain loaded until duel end.
	pub unsafe fn register_lua_function(&mut self, table: &str, name: &str,
		callback: Function) -> Result<(), RegistrationError> {
		if self.ended {
			return Err(RegistrationError::DuelEnded);
		}
		if name.is_empty() {
			return Err(RegistrationError::InvalidName);
		}
		let table = CString::new(table).map_err(|_| RegistrationError::InvalidName)?;
		let name = CString::new(name).map_err(|_| RegistrationError::InvalidName)?;
		match unsafe { ygopru_register_lua_function(self.duel_pointer,
			table.as_ptr(), name.as_ptr(), callback) } {
			1 => Ok(()),
			0 => Err(RegistrationError::MissingTable),
			_ => Err(RegistrationError::LuaError),
		}
	}
}
