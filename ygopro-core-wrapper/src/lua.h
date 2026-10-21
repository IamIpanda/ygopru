/*
 * ocgcore is compiled as C++ while lua is compiled as C, so the C++ translation units
 * need to see the lua declarations with C linkage. The lua include guards make every
 * later `#include <lua...h>` a no-op, which is why forcing this header in is enough.
 */
#ifdef __cplusplus
extern "C" {
#endif

#include <lua.h>
#include <lauxlib.h>
#include <lualib.h>

#ifdef __cplusplus
}
#endif
