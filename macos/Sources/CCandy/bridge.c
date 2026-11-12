#include "candy.h"
// A changed Rust archive must produce a different object even when its C ABI
// stays the same; SwiftPM can then invalidate the final link.
const char candy_rust_build_id[] = CANDY_RUST_BUILD_ID;
