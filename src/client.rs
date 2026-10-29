//! The real engine: `raven-client` (node/crates/raven-client, API.md). The
//! GTK app is written in Rust, so it calls `raven_client::call()` directly
//! instead of going through the C function `raven_call()`: the same JSON
//! envelope, the same process-wide engine, no FFI.
//!
//! The engine is linked in with the cargo feature `engine` (on by default).
//! A build without it still starts: every request is answered with
//! `NOT_INITIALIZED` ("this build doesn't include its engine"), as the macOS
//! and Windows apps do when their library is missing.
//!
//! `raven_client::call()` may block (`node.start` takes a few seconds), so it
//! only ever runs on the engine threads (`worker.rs`), never on the GTK main
//! thread. The profile is the CLI's (`RAVEN_DATA_DIR`, else `~/.raven`),
//! read by the engine on its first call.

use serde_json::Value;

use crate::api::{make_request, parse_response, ApiError, RavenEngine};

#[derive(Debug, Default)]
pub struct ClientEngine {
    _private: (),
}

impl ClientEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when this build links Raven's engine (feature `engine`).
    pub const fn linked() -> bool {
        cfg!(feature = "engine")
    }

    /// One request envelope in, one response envelope out: the same shape as
    /// `raven_call()`.
    #[cfg(feature = "engine")]
    fn raw_call(&self, request: &str) -> String {
        raven_client::call(request)
    }

    /// Without the engine: `NOT_INITIALIZED` for every request.
    #[cfg(not(feature = "engine"))]
    fn raw_call(&self, request: &str) -> String {
        let _ = request;
        serde_json::json!({
            "v": crate::api::API_VERSION,
            "ok": false,
            "error": {
                "code": crate::api::codes::NOT_INITIALIZED,
                "message": crate::strings::tr("error.engineMissing"),
                "detail": "this build of raven-desktop was made without the cargo feature \
                           `engine` (raven-client is not linked in)"
            }
        })
        .to_string()
    }
}

impl RavenEngine for ClientEngine {
    fn call(&self, op: &str, args: Value) -> Result<Value, ApiError> {
        parse_response(&self.raw_call(&make_request(op, &args)))
    }
}

// With the engine linked in, a call here would open the real profile: the
// engine's own tests (tests/engine.rs) point it at a throwaway one first.
#[cfg(all(test, not(feature = "engine")))]
mod tests {
    use super::*;
    use crate::api::codes;
    use crate::strings::tr;

    #[test]
    fn without_the_engine_every_call_fails_politely() {
        assert!(!ClientEngine::linked());
        let e = ClientEngine::new().profile_state().unwrap_err();
        assert_eq!(e.code, codes::NOT_INITIALIZED);
        assert_eq!(e.message, tr("error.engineMissing"));
        assert!(e.detail.contains("engine"));
        let e = ClientEngine::new().app_info().unwrap_err();
        assert!(e.is(codes::NOT_INITIALIZED));
    }
}
