#include "duel.h"
#include "interpreter.h"
#include <cstdint>

namespace {
struct registration {
	const char* table;
	const char* name;
	lua_CFunction callback;
};

int register_function(lua_State* state) {
	auto* entry = static_cast<registration*>(lua_touserdata(state, 1));
	lua_pushglobaltable(state);
	if(entry->table[0]) {
		lua_pushstring(state, entry->table);
		lua_rawget(state, -2);
		if(!lua_istable(state, -1)) {
			lua_pushboolean(state, 0);
			return 1;
		}
	}
	lua_pushstring(state, entry->name);
	lua_pushcfunction(state, entry->callback);
	lua_rawset(state, -3);
	lua_pushboolean(state, 1);
	return 1;
}
}

extern "C" int ygopru_register_lua_function(intptr_t pointer, const char* table,
	const char* name, lua_CFunction callback) {
	auto* state = reinterpret_cast<duel*>(pointer)->lua->lua_state;
	const int top = lua_gettop(state);
	if(!lua_checkstack(state, 2))
		return -1;
	registration entry{table, name, callback};
	lua_pushcfunction(state, register_function);
	lua_pushlightuserdata(state, &entry);
	const int status = lua_pcall(state, 1, 1, 0);
	const int result = status == LUA_OK ? lua_toboolean(state, -1) : -1;
	lua_settop(state, top);
	return result;
}

extern "C" int ygopru_lua_get_top(lua_State* state) {
	return lua_gettop(state);
}

extern "C" int64_t ygopru_lua_to_integer(lua_State* state, int index, int* valid) {
	return static_cast<int64_t>(lua_tointegerx(state, index, valid));
}

extern "C" void ygopru_lua_push_integer(lua_State* state, int64_t value) {
	lua_pushinteger(state, static_cast<lua_Integer>(value));
}

extern "C" void ygopru_lua_push_boolean(lua_State* state, int value) {
	lua_pushboolean(state, value);
}
