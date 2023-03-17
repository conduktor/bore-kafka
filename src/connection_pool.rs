use std::{
    collections::HashMap,
    sync::{Arc},
};

use indexmap::IndexMap;
use kafka_protocol::messages::{metadata_response::MetadataResponseBroker, BrokerId};
use tokio::{task::JoinHandle, sync::RwLock};

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

    pub async fn insert_broker(&mut self, broker_id: BrokerId, broker: MetadataResponseBroker) {
        let mut broker_store = self.broker_store.write().await;
        broker_store.insert(broker_id, broker);
    }

    pub async fn contains_broker(&self, broker_id: BrokerId) -> bool {
        let broker_store = self.broker_store.read().await;
        broker_store.contains_key(&broker_id)
    }

    pub async fn open_new_broker_connection_if_needed(
        &mut self,
        new_brokers: IndexMap<BrokerId, MetadataResponseBroker>,
    ) {
        for (broker_id, broker) in new_brokers {
            if !self.contains_broker(broker_id).await {
                self.insert_broker(broker_id, broker.clone()).await;

                self.add_connection(Url::new(
                    broker.host.to_string().clone(),
                    broker.port as u16,
                )).await;
            }
        }
    }

    fn connection_does_not_exist(&self, url: &Url) -> bool {
        !self.connections.contains_key(&url)
    }

    pub async fn add_connection(&mut self, url: Url) {
        if self.connection_does_not_exist(&url) {
            let local_url_to_relay = url.clone();
            let s = self.secret.clone();
            let at = self.get_auto_pointer();

            //port = 0 => to force random port
            let client =  Client::new(
                &local_url_to_relay.host.clone(),
                local_url_to_relay.port.clone(),
                &CONDUKTOR_BORE_SERVER,
                0,
                s.as_deref(),
                at,
            )
            .await
            .unwrap();

            let remote_port = client.remote_port.clone();

            tokio::spawn( 
                // Process each socket concurrently.
                client.listen_boxed()
            );

            self.connections.insert(url, remote_port);
        };
    }

    pub fn get_remote_port(&self, url: &Url) -> Option<u16> {
        self.connections.get(url).cloned()
    }
}
