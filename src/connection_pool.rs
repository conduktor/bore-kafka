use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use indexmap::IndexMap;
use kafka_protocol::messages::{metadata_response::MetadataResponseBroker, BrokerId};
use tokio::task::JoinHandle;

use crate::client::Client;

const CONDUKTOR_BORE_SERVER: &str = "bore.pub";

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct Url {
    pub host: String,
    pub port: u16,
}

impl Url {
    pub fn new(host: String, port: u16) -> Url {
        Url { host, port }
    }
}

pub struct ProxyState {
    pub connections: HashMap<Url, JoinHandle<()>>,
    secret: Option<String>,
    pub auto_pointer: Option<Arc<RwLock<ProxyState>>>,
    pub broker_store: Arc<RwLock<IndexMap<BrokerId, MetadataResponseBroker>>>,
}

impl ProxyState {
    pub fn new(secret: Option<String>) -> ProxyState {
        ProxyState {
            connections: HashMap::new(),
            secret: secret,
            auto_pointer: None,
            broker_store: Arc::new(RwLock::new(IndexMap::new())),
        }
    }

    pub fn set_auto_pointer(&mut self, auto_pointer: Arc<RwLock<ProxyState>>) {
        self.auto_pointer = Some(auto_pointer);
    }

    pub fn get_auto_pointer(&self) -> Arc<RwLock<ProxyState>> {
        self.auto_pointer.clone().expect("cannot be none")
    }

    fn connection_does_not_exist(&self, url: &Url) -> bool {
        !self.connections.contains_key(&url)
    }

    pub fn add_connection(&mut self, url: Url) {
        if self.connection_does_not_exist(&url) {
            let local_url_to_relay = url.clone();
            let s = self.secret.clone();
            let at = self.get_auto_pointer();

            let handler = tokio::spawn(async move {
                // Process each socket concurrently.
                //port = 0 => to force random port
                Client::new(
                    &local_url_to_relay.host.clone(),
                    local_url_to_relay.port.clone(),
                    &CONDUKTOR_BORE_SERVER,
                    0,
                    s.as_deref(),
                    at,
                )
                .await
                .unwrap()
                .listen()
                .await
                .unwrap();
            });

            self.connections.insert(url, handler);
        };
    }

    fn stop() {}
}
