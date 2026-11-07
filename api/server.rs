//! The Vercel Function. `vercel.json` rewrites every `/api/*` request here,
//! and the router still sees the original path.

use tower::ServiceBuilder;
use vercel_runtime::{Error, axum::VercelLayer};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set")?;
    let pool = champt::connect(&url).await?;
    let app = ServiceBuilder::new()
        .layer(VercelLayer::new())
        .service(champt::app(pool, champt::Verifier::aero(), champt::Gravatar::public()));
    vercel_runtime::run(app).await
}
