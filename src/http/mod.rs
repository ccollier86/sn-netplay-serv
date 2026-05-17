//! HTTP routing layer for the netplay relay.
//!
//! Routes translate HTTP requests into service calls and map domain errors back
//! to stable responses. They do not contain room rules or license verification
//! logic.

mod desktop_auth_headers;
mod errors;
mod routes;
mod services;

pub use routes::build_router;
pub use services::AppServices;
