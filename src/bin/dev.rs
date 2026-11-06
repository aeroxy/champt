//! The API on a local port, for development. The Vite dev server proxies
//! `/api` here.

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL").map_err(|_| "set DATABASE_URL (see .env.example)")?;
    let listen = std::env::var("LISTEN").unwrap_or_else(|_| "127.0.0.1:8787".to_owned());
    let pool = champt::connect(&url).await?;
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    println!("champt API on http://{listen}");
    axum::serve(listener, champt::app(pool, champt::Verifier::aero())).await?;
    Ok(())
}
