use std::collections::HashMap;

use fabro_acp::client::AcpClient;
use fabro_acp::config::{AcpServerSettings, AcpTransport};
use fabro_acp::connection_manager::AcpConnectionManager;
use fabro_acp::protocol::AcpContentBlock;

fn test_server_config() -> AcpServerSettings {
    let test_server = format!("{}/tests/test_acp_agent.py", env!("CARGO_MANIFEST_DIR"));
    AcpServerSettings {
        name:                 "test-agent".into(),
        transport:            AcpTransport::Stdio {
            command: vec!["python3".into(), test_server],
            env:     HashMap::new(),
        },
        startup_timeout_secs: 10,
        prompt_timeout_secs:  30,
    }
}

#[tokio::test]
async fn stdio_client_initialize() {
    let config = test_server_config();
    let client = AcpClient::new(&config).unwrap();
    let info = client.initialize(config.startup_timeout()).await.unwrap();

    assert_eq!(info.name, "test-acp-agent");
    assert_eq!(info.version, "0.1.0");
    assert!(info.capabilities.session);

    client.close().await.unwrap();
}

#[tokio::test]
async fn stdio_client_new_session() {
    let config = test_server_config();
    let client = AcpClient::new(&config).unwrap();
    client.initialize(config.startup_timeout()).await.unwrap();

    let session_id = client.new_session("/tmp").await.unwrap();
    assert!(!session_id.0.is_empty());

    client.close().await.unwrap();
}

#[tokio::test]
async fn stdio_client_prompt() {
    let config = test_server_config();
    let client = AcpClient::new(&config).unwrap();
    client.initialize(config.startup_timeout()).await.unwrap();

    let session_id = client.new_session("/tmp").await.unwrap();

    let result = client
        .prompt(&session_id, vec![AcpContentBlock::Text {
            text: "hello from fabro".into(),
        }])
        .await
        .unwrap();

    assert_eq!(
        result.stop_reason,
        fabro_acp::protocol::AcpStopReason::EndTurn
    );

    client.close().await.unwrap();
}

#[tokio::test]
async fn stdio_client_cancel() {
    let config = test_server_config();
    let client = AcpClient::new(&config).unwrap();
    client.initialize(config.startup_timeout()).await.unwrap();

    let session_id = client.new_session("/tmp").await.unwrap();

    client.cancel(&session_id).await.unwrap();

    client.close().await.unwrap();
}

#[tokio::test]
async fn connection_manager_stdio_roundtrip() {
    let config = test_server_config();
    let mut mgr = AcpConnectionManager::new();
    let results = mgr.start_servers(&[config]).await;

    assert_eq!(results.len(), 1);
    let (name, info_result) = &results[0];
    assert_eq!(name, "test-agent");
    let info = info_result.as_ref().unwrap();
    assert_eq!(info.name, "test-acp-agent");

    assert!(mgr.agent_names().iter().any(|n| n.contains("test_agent")));

    mgr.close_all().await;
}
