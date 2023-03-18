use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use indexmap::IndexMap;
use kafka_protocol::messages::{metadata_response::MetadataResponseBroker, BrokerId};

use crate::client::Client;

/// bore server
pub const CONDUKTOR_BORE_SERVER: &str = "bore.pub";

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
/// Represents a local url
pub struct Url {
    ///local/private host
    pub host: String,
    ///host port
    pub port: u16,
}

impl From<MetadataResponseBroker> for Url {
    fn from(broker: MetadataResponseBroker) -> Self {
        Url::new(broker.host.to_string(), broker.port as u16)
    }
}

impl Url {
    /// Create a new Url
    pub fn new(host: String, port: u16) -> Url {
        Url { host, port }
    }
}

/// Micro proxy state
pub struct ProxyState {
    /// mapping between local url and remote port
    pub connections: HashMap<Url, u16>,
    secret: Option<String>,
}

impl ProxyState {
    /// Create a new ProxyState
    pub fn new(secret: Option<String>) -> ProxyState {
        ProxyState {
            connections: HashMap::new(),
            secret,
        }
    }
    /// Insert a new broker in the ref list
    pub fn insert_broker(&mut self, broker_local_url: Url) {
        self.connections.insert(broker_local_url, 0);
    }

    /// Check if a broker is already in the ref list
    pub fn contains_broker(&self, broker_local_url: Url) -> bool {
        self.connections.contains_key(&broker_local_url)
    }

    /// Get the remote port for a given local url
    pub fn get_remote_port(&self, url: &Url) -> Option<u16> {
        self.connections.get(url).copied()
    }

    fn connection_does_not_exist(&self, url: &Url) -> bool {
        !self.connections.contains_key(url)
    }
}

/// Open a new connection to a broker if needed (if the broker is not already in the ref list)
pub async fn open_new_broker_connection_if_needed(
    state: &Arc<RwLock<ProxyState>>,
    new_brokers: IndexMap<BrokerId, MetadataResponseBroker>,
) {
    let mut new = vec![];

    {
        let lock = state.write().unwrap();
        for (_, broker) in new_brokers {
            let local_url = Url::from(broker);
            if !lock.contains_broker(local_url.clone()) {
                new.push(local_url);
            }
        }
    }

    for broker_local_url in new {
        add_connection(state, broker_local_url.clone()).await;
    }
}

/// Add open a new connection to the bore server (because a new broker was detected)
pub async fn add_connection(state: &Arc<RwLock<ProxyState>>, url: Url) {
    let secret = {
        let guard = state.read().unwrap();
        if !guard.connection_does_not_exist(&url) {
            return;
        }
        guard.secret.clone()
    };

    //port = 0 => to force random port
    let client = Client::new(
        &url.host,
        url.port,
        CONDUKTOR_BORE_SERVER,
        0,
        secret.as_deref(),
        Arc::clone(state),
    )
    .await
    .unwrap();

    let port = client.remote_port;
    tokio::spawn(
        // Process each socket concurrently.
        client.listen_boxed(),
    );

    state.write().unwrap().connections.insert(url, port);
}
