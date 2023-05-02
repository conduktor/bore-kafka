//! Kafka proxy implementation

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::mem::size_of;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use bytes::{Buf, BufMut, BytesMut};
use codec::LengthDelimitedCodec;
use dashmap::DashMap;
use futures_util::future::join_all;
use futures_util::{StreamExt, TryStreamExt};
use indexmap::IndexMap;
use kafka_protocol::messages::metadata_response::MetadataResponseBroker;
use kafka_protocol::messages::*;
use kafka_protocol::protocol::buf::{ByteBuf, NotEnoughBytesError};
use kafka_protocol::protocol::*;
use tokio::io;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_util::codec;
use tracing::debug;

use crate::auth::Authenticator;
use crate::client::Client;

#[derive(Debug)]
pub(crate) enum ErrorKind {
    Decode,
    Encode,
    Io(std::io::Error),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::Decode => {
                writeln!(f, "Error decoding message")
            }
            ErrorKind::Encode => {
                writeln!(f, "Error encoding message")
            }
            ErrorKind::Io(err) => {
                writeln!(f, "IoError: {}", err)
            }
        }
    }
}

impl Error for ErrorKind {}

impl From<std::io::Error> for ErrorKind {
    fn from(err: std::io::Error) -> Self {
        ErrorKind::Io(err)
    }
}

impl From<DecodeError> for ErrorKind {
    fn from(_err: DecodeError) -> Self {
        ErrorKind::Decode
    }
}

impl From<EncodeError> for ErrorKind {
    fn from(_err: EncodeError) -> Self {
        ErrorKind::Encode
    }
}

impl From<NotEnoughBytesError> for ErrorKind {
    fn from(_err: NotEnoughBytesError) -> Self {
        ErrorKind::Decode
    }
}

enum KafkaResponse {
    Metadata(i16, ResponseHeader, MetadataResponse),
    UndecodedResponse(BytesMut),
}

struct RequestKeyAndVersion {
    /// The API key of this request.
    pub api_key: ApiKey,

    /// The API version of this request.
    pub api_version: i16,
}

#[derive(Clone)]
struct KafkaServerCodec {
    length_codec: LengthDelimitedCodec,
    inflight: Arc<DashMap<i32, RequestKeyAndVersion>>,
}

impl KafkaServerCodec {
    pub fn new() -> Self {
        Self {
            length_codec: LengthDelimitedCodec::builder()
                .num_skip(0) // Do not strip frame header
                .length_adjustment(4)
                .new_codec(),
            inflight: Arc::new(DashMap::new()),
        }
    }
}

impl codec::Decoder for KafkaServerCodec {
    type Item = KafkaResponse;
    type Error = ErrorKind;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(mut bytes) = self.length_codec.decode(src)? {
            let correlation_id = bytes.peek_bytes(4..8).get_i32();
            match self.inflight.remove(&correlation_id) {
                Some((
                    _,
                    RequestKeyAndVersion {
                        api_key: ApiKey::MetadataKey,
                        api_version,
                    },
                )) => {
                    bytes.advance(size_of::<u32>()); // skip length
                    let header = ResponseHeader::decode(
                        &mut bytes,
                        MetadataResponse::header_version(api_version),
                    )?;
                    let response = MetadataResponse::decode(&mut bytes, api_version)?;
                    Ok(Some(KafkaResponse::Metadata(api_version, header, response)))
                }
                _ => Ok(Some(KafkaResponse::UndecodedResponse(bytes))),
            }
        } else {
            Ok(None)
        }
    }
}

impl codec::Encoder<KafkaResponse> for KafkaServerCodec {
    type Error = ErrorKind;

    fn encode(&mut self, item: KafkaResponse, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            KafkaResponse::Metadata(version, header, response) => {
                let mut bytes = BytesMut::new();
                header.encode(&mut bytes, MetadataResponse::header_version(version))?;
                response.encode(&mut bytes, version)?;
                // self.length_codec.encode(bytes.get_bytes(bytes.len()), dst)?;
                dst.put_u32(bytes.len() as u32);
                dst.put_slice(&bytes);
            }
            KafkaResponse::UndecodedResponse(bytes) => dst.put_slice(&bytes),
        }
        Ok(())
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
/// Represents a kafka broker host/port
struct KafkaBroker {
    ///local/private host
    pub host: String,
    ///host port
    pub port: u16,
}

impl KafkaBroker {
    /// Create a new KafkaBroker
    pub fn new(host: String, port: u16) -> KafkaBroker {
        KafkaBroker { host, port }
    }
}

impl From<&MetadataResponseBroker> for KafkaBroker {
    fn from(broker: &MetadataResponseBroker) -> Self {
        KafkaBroker::new(broker.host.to_string(), broker.port as u16)
    }
}

impl FromStr for KafkaBroker {
    type Err = String;

    fn from_str(bootstrap_server: &str) -> Result<Self, Self::Err> {
        let mut split = bootstrap_server.split(':');
        let host = split.next().ok_or("no host provided")?;
        let port = split.next().ok_or("no port provided")?;
        let port = port.parse().or(Err("invalid port"))?;
        Ok(KafkaBroker::new(host.to_string(), port))
    }
}

/// State structure for the kafka proxy.
pub struct KafkaProxy {
    /// Destination address of the server.
    pub to: String,

    /// Optional secret used to authenticate clients.
    pub auth: Option<Authenticator>,

    /// mapping between local url and remote port
    connections: RwLock<HashMap<KafkaBroker, u16>>,
}

impl KafkaProxy {
    /// Create a new kafka proxy.
    pub fn new(to: &str, secret: Option<&str>) -> Self {
        let auth = secret.map(Authenticator::new);

        Self {
            to: to.to_string(),
            auth,
            connections: HashMap::new().into(),
        }
    }

    /// Start the proxy, returning the remote bootstrap server to use for connecting.
    pub async fn start(self, bootstrap_servers: &str) -> anyhow::Result<String> {
        let this = Arc::new(self);
        let url = bootstrap_servers.parse().map_err(anyhow::Error::msg)?;
        let remote_port = this.add_connection(url).await?;
        Ok(format!("{}:{}", &this.to, remote_port))
    }

    /// proxy a connection
    pub(crate) async fn kafka_proxy<S1, S2>(
        self: &Arc<Self>,
        local: S1,
        remote: S2,
    ) -> Result<(), ErrorKind>
    where
        S1: AsyncRead + AsyncWrite + Unpin,
        S2: AsyncRead + AsyncWrite + Unpin,
    {
        let (local_read, local_write) = io::split(local);
        let (remote_read, remote_write) = io::split(remote);
        let codec = KafkaServerCodec::new();

        tokio::select! {
            res = Self::remote_to_local(remote_read, local_write, codec.clone()) => res,
            res = self.local_to_remote(local_read, remote_write, codec) => res,
        }
    }

    async fn remote_to_local<S1, S2>(
        remote_read: S1,
        mut local_write: S2,
        upstream_codec: KafkaServerCodec,
    ) -> Result<(), ErrorKind>
    where
        S1: AsyncRead + Unpin,
        S2: AsyncWrite + Unpin,
    {
        let codec = LengthDelimitedCodec::builder()
            .num_skip(0) // Do not strip frame header
            .length_adjustment(4)
            .new_codec();

        let mut source = codec::FramedRead::new(remote_read, codec);

        while let Some(mut bytes) = source.try_next().await? {
            let api_key = bytes.peek_bytes(4..6).get_i16();
            debug!("api_key: {}", api_key);
            if api_key == ApiKey::MetadataKey as i16 {
                let api_version = bytes.peek_bytes(6..8).get_i16();
                let correlation_id = bytes.peek_bytes(8..12).get_i32();
                debug!("api_version: {}", api_version);
                debug!("correlation_id: {}", correlation_id);

                upstream_codec.inflight.insert(
                    correlation_id,
                    RequestKeyAndVersion {
                        api_key: ApiKey::MetadataKey,
                        api_version,
                    },
                );
            };
            local_write.write_all_buf(&mut bytes).await?;
        }

        Ok(())
    }

    async fn local_to_remote<S1, S2>(
        self: &Arc<Self>,
        local_read: S1,
        remote_write: S2,
        codec: KafkaServerCodec,
    ) -> Result<(), ErrorKind>
    where
        S1: AsyncRead + Unpin,
        S2: AsyncWrite + Unpin,
    {
        let source = codec::FramedRead::new(local_read, codec);
        let sink = codec::FramedWrite::new(remote_write, KafkaServerCodec::new());

        source
            .then(|item| {
                let this = Arc::clone(self);
                async move {
                    match item {
                        Ok(KafkaResponse::Metadata(version, header, response)) => {
                            Ok(KafkaResponse::Metadata(
                                version,
                                header,
                                this.adapt_metadata(response).await,
                            ))
                        }
                        other => other,
                    }
                }
            })
            .forward(sink)
            .await?;
        Ok(())
    }

    async fn adapt_metadata(self: &Arc<Self>, mut metadata: MetadataResponse) -> MetadataResponse {
        self.open_new_broker_connection_if_needed(&metadata.brokers)
            .await;

        let connections = self.connections.read().unwrap();
        for broker in metadata.brokers.values_mut() {
            debug!("broker: {:?}", broker);
            let url = KafkaBroker::new(broker.host.to_string(), broker.port as u16);
            broker.host = unsafe { StrBytes::from_utf8_unchecked(self.to.clone().into()) }; // self.to is a String so it's safe, but the api is lacking this conversion.
            broker.port = *connections.get(&url).unwrap() as i32;
        }
        metadata
    }

    /// Open a new connection to a broker if needed (if the broker is not already in the ref list)
    async fn open_new_broker_connection_if_needed(
        self: &Arc<Self>,
        brokers: &IndexMap<BrokerId, MetadataResponseBroker>,
    ) {
        let mut unknown_brokers = vec![];

        {
            let connections = self.connections.read().unwrap();
            for broker in brokers.values() {
                let local_url = KafkaBroker::from(broker);
                if !connections.contains_key(&local_url) {
                    unknown_brokers.push(local_url);
                }
            }
        }

        join_all(
            unknown_brokers
                .into_iter()
                .map(|url| self.add_connection(url)),
        )
        .await;
    }

    /// Add open a new connection to the bore server (because a new broker was detected)
    async fn add_connection(self: &Arc<Self>, url: KafkaBroker) -> anyhow::Result<u16> {
        let client = Client::new(&url.host, url.port, Arc::clone(self)).await?;

        let remote_port = client.remote_port();
        self.connections.write().unwrap().insert(url, remote_port);

        tokio::spawn(
            // Process each socket concurrently.
            client.listen_boxed(),
        );
        Ok(remote_port)
    }
}
