use std::process::Stdio;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, CancelNotification, Client as AcpClientTrait, ClientCapabilities, ClientSideConnection,
    ContentBlock, Implementation, InitializeRequest, NewSessionRequest, PermissionOptionId,
    PromptRequest, ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionId as AcpSessionIdType,
    SessionNotification, StopReason as AcpStopReasonType, TextContent,
};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::{LocalSet, spawn_local};
use tokio::time::timeout;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, info};

use crate::config::{AcpServerSettings, AcpTransport};
use crate::error::AcpError;
use crate::protocol::{
    AcpAgentInfo, AcpCapabilities, AcpContentBlock, AcpPromptResult, AcpSessionId, AcpStopReason,
};

enum ClientState {
    Disconnected,
    Ready(Arc<ClientSideConnection>),
    Closed,
}

#[derive(Clone)]
struct FabroAcpClientHandler;

#[async_trait::async_trait(?Send)]
impl AcpClientTrait for FabroAcpClientHandler {
    async fn request_permission(
        &self,
        _args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        Ok(RequestPermissionResponse::new(
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                PermissionOptionId::new("allow"),
            )),
        ))
    }

    async fn session_notification(
        &self,
        _args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        Ok(())
    }
}

pub struct AcpClient {
    server_name: String,
    state:       Mutex<ClientState>,
    child:       Mutex<Option<Child>>,
    local_set:   LocalSet,
}

impl AcpClient {
    pub fn new(config: &AcpServerSettings) -> Result<Self, AcpError> {
        let (program, args, env) = match &config.transport {
            AcpTransport::Stdio { command, env } => {
                let (prog, a) = command.split_first().ok_or_else(|| {
                    AcpError::Connection(format!(
                        "ACP server '{}': command must not be empty",
                        config.name
                    ))
                })?;
                (prog.clone(), a.to_vec(), env.clone())
            }
            AcpTransport::Http { .. } | AcpTransport::Sandbox { .. } => {
                return Err(AcpError::Connection(format!(
                    "ACP server '{}': {:?} transport is not yet supported",
                    config.name, config.transport
                )));
            }
        };

        let mut cmd = Command::new(program);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if !env.is_empty() {
            cmd.envs(&env);
        }

        #[cfg(unix)]
        cmd.process_group(0);

        let child = cmd.spawn().map_err(|e| {
            AcpError::Connection(format!(
                "failed to spawn ACP server '{}': {}",
                config.name, e
            ))
        })?;

        debug!(server = %config.name, "Creating ACP client");

        Ok(Self {
            server_name: config.name.clone(),
            state:       Mutex::new(ClientState::Disconnected),
            child:       Mutex::new(Some(child)),
            local_set:   LocalSet::new(),
        })
    }

    async fn ensure_connected(&self) -> Result<Arc<ClientSideConnection>, AcpError> {
        {
            let guard = self.state.lock().await;
            if let ClientState::Ready(conn) = &*guard {
                return Ok(Arc::clone(conn));
            }
            if let ClientState::Closed = &*guard {
                return Err(AcpError::Connection("client is closed".into()));
            }
        }

        self.connect_inner().await
    }

    async fn connect_inner(&self) -> Result<Arc<ClientSideConnection>, AcpError> {
        let (child_stdin, child_stdout) = {
            let mut child_guard = self.child.lock().await;
            let child = child_guard
                .as_mut()
                .ok_or_else(|| AcpError::Connection("ACP server process not available".into()))?;

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| AcpError::Connection("failed to acquire stdin".into()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| AcpError::Connection("failed to acquire stdout".into()))?;

            (stdin, stdout)
        };

        let local_set = &self.local_set;
        let conn = local_set
            .run_until(Self::setup_connection(child_stdin, child_stdout))
            .await?;

        let mut state_guard = self.state.lock().await;
        *state_guard = ClientState::Ready(Arc::clone(&conn));

        Ok(conn)
    }

    #[allow(clippy::unused_async)]
    async fn setup_connection(
        stdin: ChildStdin,
        stdout: ChildStdout,
    ) -> Result<Arc<ClientSideConnection>, AcpError> {
        let handler = FabroAcpClientHandler;
        let (connection, io_future) =
            ClientSideConnection::new(handler, stdin.compat_write(), stdout.compat(), |fut| {
                spawn_local(fut);
            });

        spawn_local(io_future);

        Ok(Arc::new(connection))
    }

    pub async fn initialize(
        &self,
        init_timeout: std::time::Duration,
    ) -> Result<AcpAgentInfo, AcpError> {
        let conn = self.ensure_connected().await?;

        let client_info = Implementation::new("fabro-acp", env!("CARGO_PKG_VERSION"));
        let request = InitializeRequest::new(ProtocolVersion::LATEST)
            .client_info(client_info)
            .client_capabilities(ClientCapabilities::new());

        let local_set = &self.local_set;
        let result = timeout(init_timeout, local_set.run_until(conn.initialize(request)))
            .await
            .map_err(|_| {
                AcpError::Timeout(format!(
                    "timed out initializing ACP server '{}' after {:?}",
                    self.server_name, init_timeout
                ))
            })?
            .map_err(|e| {
                AcpError::Initialization(format!(
                    "failed to initialize ACP server '{}': {}",
                    self.server_name, e
                ))
            })?;

        let (name, version) = result.agent_info.as_ref().map_or_else(
            || ("unknown".to_string(), "0.0.0".to_string()),
            |info| (info.name.clone(), info.version.clone()),
        );

        let agent_info = AcpAgentInfo {
            name:         name.clone(),
            version:      version.clone(),
            capabilities: AcpCapabilities {
                prompt:  result.agent_capabilities.prompt_capabilities.image
                    || result.agent_capabilities.prompt_capabilities.audio
                    || result
                        .agent_capabilities
                        .prompt_capabilities
                        .embedded_context,
                session: result
                    .agent_capabilities
                    .session_capabilities
                    .list
                    .is_some(),
                auth:    !result.auth_methods.is_empty(),
            },
        };

        info!(
            server = %self.server_name,
            agent_name = %name,
            agent_version = %version,
            "ACP server initialized"
        );

        Ok(agent_info)
    }

    pub async fn new_session(&self, cwd: &str) -> Result<AcpSessionId, AcpError> {
        let conn = self.ensure_connected().await?;

        let request = NewSessionRequest::new(cwd);

        let local_set = &self.local_set;
        let result = local_set
            .run_until(conn.new_session(request))
            .await
            .map_err(|e| {
                AcpError::Session(format!(
                    "failed to create session on ACP server '{}': {}",
                    self.server_name, e
                ))
            })?;

        let session_id = AcpSessionId(result.session_id.0.to_string());

        debug!(
            server = %self.server_name,
            session_id = %session_id.0,
            "Created ACP session"
        );

        Ok(session_id)
    }

    pub async fn prompt(
        &self,
        session_id: &AcpSessionId,
        content: Vec<AcpContentBlock>,
    ) -> Result<AcpPromptResult, AcpError> {
        let conn = self.ensure_connected().await?;

        let acp_content: Vec<ContentBlock> = content
            .into_iter()
            .map(|block| match block {
                AcpContentBlock::Text { text } => ContentBlock::Text(TextContent::new(text)),
            })
            .collect();

        let request = PromptRequest::new(AcpSessionIdType::new(session_id.0.as_str()), acp_content);

        let local_set = &self.local_set;
        let result = local_set
            .run_until(conn.prompt(request))
            .await
            .map_err(|e| {
                AcpError::Prompt(format!(
                    "failed to prompt ACP server '{}': {}",
                    self.server_name, e
                ))
            })?;

        let stop_reason = match result.stop_reason {
            AcpStopReasonType::EndTurn => AcpStopReason::EndTurn,
            AcpStopReasonType::Cancelled => AcpStopReason::Cancelled,
            _ => AcpStopReason::Error(format!("{:?}", result.stop_reason)),
        };

        Ok(AcpPromptResult {
            text: String::new(),
            stop_reason,
            files_touched: Vec::new(),
        })
    }

    pub async fn cancel(&self, session_id: &AcpSessionId) -> Result<(), AcpError> {
        let conn = self.ensure_connected().await?;

        let notification = CancelNotification::new(AcpSessionIdType::new(session_id.0.as_str()));

        let local_set = &self.local_set;
        local_set
            .run_until(conn.cancel(notification))
            .await
            .map_err(|e| {
                AcpError::Session(format!(
                    "failed to cancel session on ACP server '{}': {}",
                    self.server_name, e
                ))
            })?;

        Ok(())
    }

    pub async fn close(&self) -> Result<(), AcpError> {
        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            let _ = child.kill().await;
        }
        *child_guard = None;

        let mut state_guard = self.state.lock().await;
        *state_guard = ClientState::Closed;

        Ok(())
    }
}
