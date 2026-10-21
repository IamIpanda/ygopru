#include "duel.h"
#include "interpreter.h"

extern "C" lua_State* ygocore_lua_state(intptr_t pduel) {
	return reinterpret_cast<duel*>(pduel)->lua->lua_state;
}
