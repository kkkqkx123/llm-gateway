use llm_gateway::config::Config;
use llm_gateway::logging::initialize_logging;
use llm_gateway::server::HttpServer;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_logging()?;

    let config = Config::from_file("config.yaml").unwrap_or_else(|_| {
        info!("Using default configuration");
        Config::default()
    });

    let server = HttpServer::new(config);
    server.run().await?;

    Ok(())
}
