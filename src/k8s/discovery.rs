use super::client::K8sClient;
use k8s_openapi::api::core::v1::Pod;
use log::info;
use std::error::Error;

pub struct DiscoveredContainer {
    pub pod_name: String,
    pub namespace: String,
    pub container_name: String,
    pub node_name: String,
}

// Discover pods in namespace
pub async fn discover_pods(
    client: &K8sClient,
    namespace: &str,
) -> Result<Vec<Pod>, Box<dyn Error>> {
    client.list_pods(namespace).await
}

// Discovers containers in namespace, optionally filtering it by labels selectors
pub async fn discover_containers(
    client: &K8sClient,
    namespace: &str,
    label_selector: Option<&str>,
) -> Result<Vec<DiscoveredContainer>, Box<dyn Error>> {
    let pods = client.list_pods(namespace).await?;

    let mut containers: Vec<DiscoveredContainer> = Vec::new();
    for pod in pods {
        let pod_name = pod.metadata.name.clone().unwrap_or_default();
        let pod_namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let node_name = pod
            .spec
            .as_ref()
            .and_then(|ps| ps.node_name.clone())
            .unwrap_or_default();

        // Apply label selector if provided
        if let Some(selector) = label_selector {
            if !matches_labels(&pod, selector) {
                continue;
            }
        }

        // Extract container info
        if let Some(spec) = pod.spec {
            for container in spec.containers {
                let container_name = container.name;
                containers.push(DiscoveredContainer {
                    pod_name: pod_name.clone(),
                    namespace: pod_namespace.clone(),
                    container_name,
                    node_name: node_name.clone(),
                });
            }
        }
    }

    info!(
        "Discovered {} containers in namespace {}",
        containers.len(),
        namespace
    );
    Ok(containers)
}

// Check if a pod matches the given label selector
fn matches_labels(pod: &Pod, selector: &str) -> bool {
    // Implementing very simple logic: Can be modified for more complex selectors
    if let Some(labels) = pod.metadata.labels.as_ref() {
        let pairs: Vec<&str> = selector.split(",").collect();
        for pair in pairs {
            let kv: Vec<&str> = pair.split('=').collect();
            if kv.len() == 2 {
                let key = kv[0];
                let value = kv[1];
                if !labels.contains_key(key) || labels[key] != value {
                    return false;
                }
            }
        }
        return true;
    }

    return false;
}
