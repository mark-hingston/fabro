use std::sync::Arc;
use std::time::Duration;

use fabro_acp::config::AcpServerSettings;
use fabro_acp::connection_manager::{AcpConnectionManager, parse_qualified_name};
use fabro_acp::protocol::{AcpContentBlock, AcpPromptResult, AcpSessionId};
use fabro_llm::types::ToolDefinition;
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;
use tokio::time::timeout;
use tracing::debug;

use crate::tool_registry::RegisteredTool;

enum AcpRequest {
    Prompt {
        server_name: String,
        session_id:  AcpSessionId,
        content:     Vec<AcpContentBlock>,
        timeout:     Duration,
        reply:       oneshot::Sender<Result<AcpPromptResult, String>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

#[derive(Clone)]
pub struct AcpHandle {
    tx: mpsc::Sender<AcpRequest>,
}

impl AcpHandle {
    pub async fn prompt(
        &self,
        server_name: &str,
        session_id: &AcpSessionId,
        content: Vec<AcpContentBlock>,
        timeout: Duration,
    ) -> Result<AcpPromptResult, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(AcpRequest::Prompt {
                server_name: server_name.to_string(),
                session_id: session_id.clone(),
                content,
                timeout,
                reply: reply_tx,
            })
            .await
            .map_err(|_| "ACP connection manager task shut down".to_string())?;
        reply_rx
            .await
            .map_err(|_| "ACP reply channel dropped".to_string())?
    }
}

pub struct AcpRunner {
    handle:  AcpHandle,
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl AcpRunner {
    #[must_use]
    pub fn handle(&self) -> AcpHandle {
        self.handle.clone()
    }
}

impl Drop for AcpRunner {
    fn drop(&mut self) {
        let _ = self.handle.tx.try_send(AcpRequest::Shutdown {
            reply: oneshot::channel().0,
        });
    }
}

pub fn make_acp_tools(handle: &AcpHandle, agent_names: Vec<String>) -> Vec<RegisteredTool> {
    agent_names
        .into_iter()
        .map(|qualified_name| {
            let h = handle.clone();
            let server_name = parse_qualified_name(&qualified_name)
                .map_or_else(|| qualified_name.clone(), |(s, _a)| s);
            let agent_name = parse_qualified_name(&qualified_name)
                .map_or_else(|| "agent".to_string(), |(_s, a)| a);
            #[allow(clippy::duration_suboptimal_units)]
            let tool_timeout = Duration::from_secs(120);

            let description =
                format!("Send a prompt to the ACP agent '{agent_name}' (server: {server_name})");

            let sn = server_name.clone();
            RegisteredTool {
                definition: ToolDefinition {
                    name: qualified_name,
                    description,
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "prompt": {
                                "type": "string",
                                "description": "The task to delegate to the ACP agent"
                            }
                        },
                        "required": ["prompt"]
                    }),
                },
                executor:   Arc::new(move |args, _ctx| {
                    let h = h.clone();
                    let server_name = sn.clone();
                    let timeout = tool_timeout;
                    Box::pin(async move {
                        let prompt_text = args
                            .get("prompt")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| "missing required parameter 'prompt'".to_string())?
                            .to_string();

                        let session_id =
                            AcpSessionId(format!("fabro-tool-{}", uuid::Uuid::new_v4()));
                        let content = vec![AcpContentBlock::Text { text: prompt_text }];

                        let result = h
                            .prompt(&server_name, &session_id, content, timeout)
                            .await?;

                        Ok(result.text)
                    })
                }),
            }
        })
        .collect()
}

struct AcpInitResult {
    handle:          AcpHandle,
    agent_names:     Vec<String>,
    startup_results: Vec<(String, Result<usize, String>)>,
}

pub async fn start_acp_servers(
    configs: Vec<AcpServerSettings>,
) -> (AcpRunner, Vec<String>, Vec<(String, Result<usize, String>)>) {
    if configs.is_empty() {
        let (tx, _rx) = mpsc::channel::<AcpRequest>(1);
        let handle = AcpHandle { tx };
        return (
            AcpRunner {
                handle:  handle.clone(),
                _thread: None,
            },
            vec![],
            vec![],
        );
    }

    let (init_tx, init_rx) = oneshot::channel::<AcpInitResult>();

    let configs_clone = configs;
    #[expect(
        clippy::disallowed_methods,
        reason = "ACP needs a dedicated OS thread for LocalSet"
    )]
    let thread = std::thread::spawn(move || {
        let rt = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build ACP runtime");
        let local = LocalSet::new();

        rt.block_on(local.run_until(async move {
            let mut manager = AcpConnectionManager::new();
            let results = manager.start_servers(&configs_clone).await;
            let agent_names = manager.agent_names();

            let startup_results: Vec<(String, Result<usize, String>)> = results
                .into_iter()
                .map(|(name, result)| match result {
                    Ok(_) => (name, Ok(1)),
                    Err(e) => (name, Err(e.to_string())),
                })
                .collect();

            let (tx, mut rx) = mpsc::channel::<AcpRequest>(64);
            let handle = AcpHandle { tx };

            let _ = init_tx.send(AcpInitResult {
                handle: handle.clone(),
                agent_names,
                startup_results,
            });

            while let Some(req) = rx.recv().await {
                match req {
                    AcpRequest::Prompt {
                        server_name,
                        session_id,
                        content,
                        timeout: prompt_timeout,
                        reply,
                    } => {
                        let result = timeout(
                            prompt_timeout,
                            manager.prompt(&server_name, &session_id, content),
                        )
                        .await;
                        let response = match result {
                            Ok(Ok(prompt_result)) => Ok(prompt_result),
                            Ok(Err(e)) => Err(e.to_string()),
                            Err(_) => Err(format!(
                                "ACP prompt to '{server_name}' timed out after {prompt_timeout:?}"
                            )),
                        };
                        let _ = reply.send(response);
                    }
                    AcpRequest::Shutdown { reply } => {
                        manager.close_all().await;
                        let _ = reply.send(());
                        break;
                    }
                }
            }
            debug!("ACP connection manager task finished");
        }));
    });

    let init = init_rx.await.expect("ACP init thread panicked");

    let runner = AcpRunner {
        handle:  init.handle,
        _thread: Some(thread),
    };

    (runner, init.agent_names, init.startup_results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_handle() -> (AcpHandle, mpsc::Receiver<AcpRequest>) {
        let (tx, rx) = mpsc::channel::<AcpRequest>(64);
        (AcpHandle { tx }, rx)
    }

    #[test]
    fn make_acp_tools_creates_tools_for_each_agent() {
        let (handle, _rx) = test_handle();

        let agent_names = vec![
            "acp__copilot__codegen".to_string(),
            "acp__my_agent__code_gen".to_string(),
        ];

        let tools = make_acp_tools(&handle, agent_names);

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].definition.name, "acp__copilot__codegen");
        assert_eq!(tools[1].definition.name, "acp__my_agent__code_gen");
    }

    #[test]
    fn acp_tool_description_mentions_agent_name() {
        let (handle, _rx) = test_handle();

        let tools = make_acp_tools(&handle, vec!["acp__copilot__codegen".to_string()]);
        assert!(tools[0].definition.description.contains("codegen"));
        assert!(tools[0].definition.description.contains("copilot"));
    }

    #[test]
    fn acp_tool_has_required_prompt_parameter() {
        let (handle, _rx) = test_handle();

        let tools = make_acp_tools(&handle, vec!["acp__server__agent".to_string()]);
        let params = &tools[0].definition.parameters;

        assert_eq!(params["required"][0], "prompt");
        assert_eq!(params["properties"]["prompt"]["type"], "string");
    }

    #[test]
    fn make_acp_tools_empty_agents() {
        let (handle, _rx) = test_handle();

        let tools = make_acp_tools(&handle, vec![]);
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn acp_runner_starts_and_drops_cleanly() {
        let (runner, _, _) = start_acp_servers(vec![]).await;
        drop(runner);
    }

    #[tokio::test]
    async fn acp_runner_handles_prompt_for_missing_server() {
        let (runner, _, _) = start_acp_servers(vec![]).await;
        let handle = runner.handle();

        let result = handle
            .prompt(
                "nonexistent",
                &AcpSessionId("test".to_string()),
                vec![AcpContentBlock::Text {
                    text: "hello".to_string(),
                }],
                Duration::from_secs(5),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn acp_handle_prompt_fails_when_channel_closed() {
        let handle = {
            let (tx, _rx) = mpsc::channel::<AcpRequest>(64);
            AcpHandle { tx }
        };

        let result = handle
            .prompt(
                "any",
                &AcpSessionId("test".to_string()),
                vec![AcpContentBlock::Text {
                    text: "hello".to_string(),
                }],
                Duration::from_secs(5),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn start_acp_servers_with_no_configs() {
        let (runner, agent_names, results) = start_acp_servers(vec![]).await;
        assert!(agent_names.is_empty());
        assert!(results.is_empty());
        drop(runner);
    }

    #[tokio::test]
    async fn acp_settings_deserialize_and_create_empty_runner() {
        use fabro_acp::config::AcpServerSettings;

        let settings = AcpServerSettings::default();
        let deserialized: AcpServerSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(deserialized.name, settings.name);

        let (runner, agent_names, results) = start_acp_servers(vec![]).await;
        assert!(agent_names.is_empty());
        assert!(results.is_empty());
        drop(runner);
    }
}
