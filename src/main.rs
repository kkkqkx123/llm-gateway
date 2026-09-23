use llm_gateway::config::Config;
use llm_gateway::logging::initialize_logging;
use llm_gateway::server::HttpServer;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_logging()?;

    let config_path = "config.yaml";
    let server = match HttpServer::from_config_file(config_path) {
        Ok(s) => {
            info!("Loaded configuration from {}", config_path);
            s
        }
        Err(_) => {
            info!("config.yaml not found or invalid — using default configuration");
            HttpServer::default()
        }
    };

    server.run().await?;

    Ok(())
}
