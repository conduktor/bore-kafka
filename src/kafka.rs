use std::error::Error;
use std::fmt::{Display, Formatter};
use std::mem::size_of;
use std::sync::{Arc};

use bytes::Buf;
use bytes::{BufMut, BytesMut};
use codec::LengthDelimitedCodec;
use dashmap::DashMap;
use futures_util::{StreamExt, TryStreamExt};
use indexmap::IndexMap;
use kafka_protocol::messages::metadata_response::MetadataResponseBroker;
use kafka_protocol::messages::*;
use kafka_protocol::protocol::buf::{ByteBuf, NotEnoughBytesError};
use kafka_protocol::protocol::{
    types, Decodable, DecodeError, Encodable, EncodeError, HeaderVersion, StrBytes,
};
use tokio::io;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio_util::codec;
use tracing::{debug, info};

use crate::connection_pool::{ProxyState, Url};

#[derive(Debug)]
pub enum ErrorKind {
    DecodeError,
    EncodeError,
    UnsupportedOperation,
    IoError(std::io::Error),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::DecodeError => {
                writeln!(f, "Error decoding message")
            }
            ErrorKind::EncodeError => {
                writeln!(f, "Error encoding message")
            }
            ErrorKind::UnsupportedOperation => {
                writeln!(f, "Unsupported API")
            }
            ErrorKind::IoError(err) => {
                writeln!(f, "IoError: {}", err)
            }
        }
    }
}

impl Error for ErrorKind {}

impl From<std::io::Error> for ErrorKind {
    fn from(err: std::io::Error) -> Self {
        ErrorKind::IoError(err)
    }
}

impl From<DecodeError> for ErrorKind {
    fn from(_err: DecodeError) -> Self {
        ErrorKind::DecodeError
    }
}

impl From<EncodeError> for ErrorKind {
    fn from(_err: EncodeError) -> Self {
        ErrorKind::EncodeError
    }
}

impl From<()> for ErrorKind {
    fn from(_: ()) -> Self {
        ErrorKind::DecodeError
    }
}

impl From<NotEnoughBytesError> for ErrorKind {
    fn from(_err: NotEnoughBytesError) -> Self {
        ErrorKind::DecodeError
    }
}

pub enum KafkaResponse {
    Metadata(i16, ResponseHeader, MetadataResponse),
    UndecodedResponse(BytesMut),
}

pub struct RequestKeyAndVersion {
    /// The API key of this request.
    pub api_key: ApiKey,

    /// The API version of this request.
    pub api_version: i16,
}

#[derive(Clone)]
pub struct KafkaServerCodec {
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

pub async fn kafka_proxy<S1, S2>(
    local: S1,
    remote: S2,
    proxy_state: Arc<RwLock<ProxyState>>,
) -> Result<(), ErrorKind>
where
    S1: AsyncRead + AsyncWrite + Unpin,
    S2: AsyncRead + AsyncWrite + Unpin,
{
    let (local_read, local_write) = io::split(local);
    let (remote_read, remote_write) = io::split(remote);
    let codec = KafkaServerCodec::new();

    let broker_store: Arc<RwLock<IndexMap<BrokerId, MetadataResponseBroker>>> =
        Arc::new(RwLock::new(IndexMap::new()));

    tokio::select! {
        res = remote_to_local(remote_read, local_write, codec.clone()) => res,
        res = local_to_remote(local_read, remote_write, codec,proxy_state) => res,
    }
}

pub async fn remote_to_local<S1, S2>(
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

pub fn adapt_metadata(
    mut metadata: MetadataResponse,
    proxy_state: Arc<RwLock<ProxyState>>,
) -> MetadataResponse {
    let new_brokers: IndexMap<BrokerId, MetadataResponseBroker> = metadata.brokers.clone().into();

    //let mut lock = proxy_state.blocking_write();

    // let inner_state  = proxy_state.clone();

    // lock.open_new_broker_connection_if_needed(new_brokers).await;
    //tokio::task::spawn_blocking(move || {
    //    let mut inner_lock = inner_state.write().unwrap();
    //    futures::executor::block_on(inner_lock.open_new_broker_connection_if_needed(new_brokers))
    //});

    Handle::current().block_on(tokio::spawn(async move {
        let mut lock = proxy_state.write().await;
        lock.open_new_broker_connection_if_needed(new_brokers).await;
        info!("mapping: {:?}", lock.connections);


        for broker in metadata.brokers.values_mut() {
            info!("broker: {:?}", broker);
            let url = Url::new(broker.host.to_string(), broker.port as u16);
            debug!("url: {:?}", url);


            broker.host = StrBytes::from_str("bore.pub"); // FIXME
            broker.port = lock.get_remote_port(&url)
                .unwrap() as i32;
        }
        metadata

    })).unwrap()
    //futures::executor::block_on(lock.open_new_broker_connection_if_needed(new_brokers));


    //get the port from the broker store
    //apply port mapping to the broker list
}

// pub async fn adapt_metadata_async(
//     mut metadata: MetadataResponse,
//     proxy_state: Arc<RwLock<ProxyState>>,
// ) -> MetadataResponse {
//     let new_brokers: IndexMap<BrokerId, MetadataResponseBroker> = metadata.brokers.clone().into();
//
//
//         let mut lock = proxy_state.write().await;
//         lock.open_new_broker_connection_if_needed(new_brokers).await;
//
//     //get the port from the broker store
//     //apply port mapping to the broker list
//     info!("mapping: {:?}", lock.connections);
//
//     for broker in metadata.brokers.values_mut() {
//         info!("broker: {:?}", broker);
//         broker.host = StrBytes::from_str("bore.pub"); // FIXME
//         broker.port = lock
//             .get_remote_port(&Url::new(broker.host.to_string(), broker.port as u16))
//             .unwrap() as i32;
//     }
//     metadata
// }


pub async fn local_to_remote<S1, S2>(
    local_read: S1,
    remote_write: S2,
    codec: KafkaServerCodec,
    proxy_state: Arc<RwLock<ProxyState>>,
) -> Result<(), ErrorKind>
where
    S1: AsyncRead + Unpin,
    S2: AsyncWrite + Unpin,
{
    let source = codec::FramedRead::new(local_read, codec);
    let sink = codec::FramedWrite::new(remote_write, KafkaServerCodec::new());

    source
        .map(|item| { match item {
            Ok(KafkaResponse::Metadata(version, header, response)) => Ok(KafkaResponse::Metadata(
                version,
                header,
                adapt_metadata(response, proxy_state.clone()),
            )),
            other => other,
        }})
        .forward(sink)
        .await?;
    Ok(())
}
