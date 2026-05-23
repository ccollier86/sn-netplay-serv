//! ShadowBoy netplay analytics operator CLI.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    sb_netplay_serv::analytics::run_cli().await
}
