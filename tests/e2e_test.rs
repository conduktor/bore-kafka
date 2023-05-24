use std::time::Duration;

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use rskafka::client::partition::Compression;
use rskafka::client::ClientBuilder;
use rskafka::record;
use rskafka::time::OffsetDateTime;
use rstest::*;
use testcontainers::images::kafka;
use testcontainers::images::kafka::Kafka;
use testcontainers::{clients, Container};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time;

use conduktor_kafka_proxy::kafka::KafkaProxy;
use conduktor_kafka_proxy::{server::Server, shared::CONTROL_PORT};

lazy_static! {
    /// Guard to make sure that tests are run serially, not concurrently.
    static ref SERIAL_GUARD: Mutex<()> = Mutex::new(());
}

const TEST_SECRET: &str = "a secret";

/// Spawn the server, giving some time for the control port TcpListener to start.
async fn spawn_server(secret: Option<&str>) {
    tokio::spawn(Server::new(1024, secret).listen());
    time::sleep(Duration::from_millis(50)).await;
}

/// Start a Kafka container, returning the container and its bootstrap servers.
fn start_kafka(docker: &clients::Cli) -> (Container<Kafka>, String) {
    let kafka_node = docker.run(Kafka::default());
    let local_port = kafka_node.get_host_port_ipv4(kafka::KAFKA_PORT);
    (kafka_node, format!("127.0.0.1:{}", local_port))
}

/// Spawns a client with randomly assigned ports, returning the listener and remote address.
async fn spawn_proxy(secret: Option<&str>, bootstrap_servers: &str) -> Result<String> {
    let remote = KafkaProxy::new("localhost", secret)
        .start(bootstrap_servers)
        .await?;
    Ok(remote)
}

#[rstest]
#[tokio::test]
#[cfg_attr(not(feature = "integration_tests"), ignore)]
async fn basic_proxy(#[values(None, Some(""), Some("abc"))] secret: Option<&str>) -> Result<()> {
    let _guard = SERIAL_GUARD.lock().await;
    let docker = clients::Cli::default();
    let (_kafka_node, bootstrap_servers) = start_kafka(&docker);
    spawn_server(secret).await;

    let remote = spawn_proxy(secret, &bootstrap_servers).await?;
    let bootstrap_servers = vec![remote];
    let topic = "test-topic";

    let client = ClientBuilder::new(bootstrap_servers).build().await.unwrap();
    let partition_client = client
        .partition_client(
            topic.to_owned(),
            0, // partition
        )
        .unwrap();

    let number_of_messages_to_produce = 5;
    let expected: Vec<String> = (0..number_of_messages_to_produce)
        .map(|i| format!("Message {}", i))
        .collect();

    for (i, message) in expected.iter().enumerate() {
        partition_client
            .produce(
                vec![record::Record {
                    key: Some(format!("Key {}", i).as_bytes().into()),
                    value: Some(message.as_bytes().into()),
                    headers: Default::default(),
                    timestamp: OffsetDateTime::now_utc(),
                }],
                Compression::NoCompression,
            )
            .await
            .unwrap();
    }

    let mut offset: usize = 0;
    while offset < number_of_messages_to_produce {
        let consumed = partition_client
            .fetch_records(offset as i64, 0..1_000_000, 10_000)
            .await
            .unwrap()
            .0;

        let consumed: Vec<String> = consumed
            .into_iter()
            .map(|r| String::from_utf8(r.record.value.unwrap()).unwrap())
            .collect();
        assert_eq!(consumed, expected[offset..offset + consumed.len()]);
        offset += consumed.len();
    }

    Ok(())
}

#[rstest]
#[case(None, Some("my secret"))]
#[case(Some("my secret"), None)]
#[tokio::test]
async fn mismatched_secret(
    #[case] server_secret: Option<&str>,
    #[case] client_secret: Option<&str>,
) {
    let _guard = SERIAL_GUARD.lock().await;

    spawn_server(server_secret).await;
    assert!(spawn_proxy(client_secret, "localhost:0").await.is_err());
}

#[tokio::test]
async fn invalid_address() -> Result<()> {
    // We don't need the serial guard for this test because it doesn't create a server.
    async fn check_address(to: &str, use_secret: bool) -> Result<()> {
        let result = KafkaProxy::new(to, use_secret.then_some(TEST_SECRET))
            .start("localhost:0")
            .await;
        match result {
            Ok(_) => Err(anyhow!("expected error for {to}, use_secret={use_secret}")),
            Err(_) => Ok(()),
        }
    }
    tokio::try_join!(
        check_address("google.com", false),
        check_address("google.com", true),
        check_address("nonexistent.domain.for.demonstration", false),
        check_address("nonexistent.domain.for.demonstration", true),
        check_address("malformed !$uri$%", false),
        check_address("malformed !$uri$%", true),
    )?;
    Ok(())
}

#[tokio::test]
async fn very_long_frame() -> Result<()> {
    let _guard = SERIAL_GUARD.lock().await;

    spawn_server(None).await;
    let mut attacker = TcpStream::connect(("localhost", CONTROL_PORT)).await?;

    // Slowly send a very long frame.
    for _ in 0..10 {
        let result = attacker.write_all(&[42u8; 100000]).await;
        if result.is_err() {
            return Ok(());
        }
        time::sleep(Duration::from_millis(10)).await;
    }
    panic!("did not exit after a 1 MB frame");
}
