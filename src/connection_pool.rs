use std::collections::HashMap;
use std::sync::{RwLock, Arc};

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
    pub connections: HashMap<Url, u16>,
    secret: Option<String>,
    pub broker_store: IndexMap<BrokerId, MetadataResponseBroker>,
}

impl ProxyState {
    pub fn new(secret: Option<String>) -> ProxyState {
        ProxyState {
            connections: HashMap::new(),
            secret,
            broker_store: IndexMap::new(),
        }
    }

    pub fn insert_broker(&mut self, broker_id: BrokerId, broker: MetadataResponseBroker) {
        self.broker_store.insert(broker_id, broker);
    }

    pub fn contains_broker(&self, broker_id: BrokerId) -> bool {
        self.broker_store.contains_key(&broker_id)
    }

    pub fn get_remote_port(&self, url: &Url) -> Option<u16> {
        self.connections.get(url).copied()
    }

    fn connection_does_not_exist(&self, url: &Url) -> bool {
        !self.connections.contains_key(&url)
    }
}

pub async fn open_new_broker_connection_if_needed(
    state: &Arc<RwLock<ProxyState>>,
    new_brokers: IndexMap<BrokerId, MetadataResponseBroker>,
) {
    let mut new = vec![];

    {
        let mut lock = state.write().unwrap();
        for (broker_id, broker) in new_brokers {
            if !lock.contains_broker(broker_id) {
                lock.insert_broker(broker_id, broker.clone());
                new.push(broker);
            }
        }
    }
    for broker in new {
        let url = Url::new(broker.host.to_string(), broker.port as u16);
        add_connection(state, url).await;
    }

}

pub async fn add_connection(
    state: &Arc<RwLock<ProxyState>>,
    url: Url) {
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
        &CONDUKTOR_BORE_SERVER,
        0,
        secret.as_deref(),
        Arc::clone(state),
    )
        .await
        .unwrap();

    let port = client.remote_port;
    tokio::spawn(
        // Process each socket concurrently.
        client.listen_boxed()
    );

    state.write().unwrap().connections.insert(url, port);
}
