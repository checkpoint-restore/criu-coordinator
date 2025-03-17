use super::discovery::DiscoveredContainer;
use log::*;
use reqwest::{Client as HttpClient, StatusCode};
use std::{error::Error, time::Duration};

// Trigger checkpoint for a container using the kubelet API
pub async fn trigger_checkpoint(
    container: &DiscoveredContainer,
    node_address: &str,
    cert_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let kubelet_port = 10250;
    let url = format!(
        "https://{}:{}/checkpoint/{}/{}/{}",
        node_address,
        kubelet_port,
        container.namespace,
        container.pod_name,
        container.container_name
    );

    info!("Triggering checkpoint via kubelet API: {}", url);

    let client_builder = HttpClient::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(true);

    let client = if let (Some(cert), Some(key)) = (cert_path, key_path) {
        // load client certificate and key for the kubelet authentication
        let cert = std::fs::read(cert)?;
        let key = std::fs::read(key)?;
        let identity = reqwest::Identity::from_pem(&[cert, key].concat())?;
        client_builder.identity(identity).build()?
    } else {
        client_builder.build()?
    };

    let response = client.post(&url).send().await?;

    match response.status() {
        StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED => {
            info!(
                "Checkpoint successfully triggered for {}/{}/{}",
                container.namespace, container.pod_name, container.container_name
            );
            Ok(())
        }
        status => {
            let error_msg = format!(
                "Failed to trigger checkpoint: HTTP {} - {}",
                status,
                response.text().await?
            );
            error!("{}", error_msg);
            Err(error_msg.into())
        }
    }
}

// Gets the status of a checkpoint
pub async fn get_checkpoint_status(
    container: &DiscoveredContainer,
    node_address: &str,
) -> Result<String, Box<dyn Error>> {
    // This is a placeholder since the kubelet API doesn't expose
    // a direct endpoint for checking checkpoint status.
    // In a real implementation, we would check for the presence of
    // the checkpoint file in the node's filesystem.

    // For now, return a mock status
    info!(
        "Checking checkpoint status for {}/{}/{}",
        container.namespace, container.pod_name, container.container_name
    );

    Ok("unknown".to_string())
}
