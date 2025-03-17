use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{Api, ListParams},
    Client, Config,
};
use log::*;
use std::{convert::TryFrom, error::Error};

pub struct K8sClient {
    client: Client,
}

impl K8sClient {
    // create a new client
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let config = Config::infer().await?;
        let client = Client::try_from(config)?;
        // Can use Client::new()
        Ok(Self { client })
    }

    // Get kube-client
    pub fn client(&self) -> &Client {
        &self.client
    }

    // List pods in namespace
    pub async fn list_pods(&self, namespace: &str) -> Result<Vec<Pod>, Box<dyn Error>> {
        let api: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let pods = api.list(&ListParams::default()).await?;

        info!("Found {} pods in namespace {}", pods.items.len(), namespace);
        Ok(pods.items)
    }
}
