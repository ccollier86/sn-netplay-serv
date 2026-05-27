//! HTTP routing layer for the netplay relay.
//!
//! Routes translate HTTP requests into service calls and map domain errors back
//! to stable responses. They do not contain room rules or license verification
//! logic.

mod admin_auth;
mod client_auth_headers;
mod client_identity;
mod errors;
mod lobby_routes;
mod routes;
mod services;

pub use admin_auth::AdminAuthorizer;
pub use routes::build_router;
pub use services::{AppServices, FileRelayPolicy};
