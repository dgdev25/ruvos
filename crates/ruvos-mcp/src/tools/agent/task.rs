use crate::{paths, Result, RuvosError};
use super::artifact::build_artifact;

pub(super) struct TaskOutcome {
    pub(super) artifact_path: String,
    pub(super) bytes: u64,
    pub(super) result: String,
    pub(super) success: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) stream: Option<(u64, u64)>,
    pub(super) content: String,
}

pub(super) async fn run_task(
    agent_id: &str,
    archetype: &str,
    prompt: &str,
    runner: Option<&str>,
    output_schema: Option<serde_json::Value>,
) -> Result<TaskOutcome> {
    let dir = paths::data_root().join("agents").join(agent_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| RuvosError::InternalError(format!("agent dir: {}", e)))?;

    let artifact = dir.join("output.md");
    let content = build_artifact(archetype, prompt, output_schema.as_ref());
    tokio::fs::write(&artifact, &content)
        .await
        .map_err(|e| RuvosError::InternalError(format!("write artifact: {}", e)))?;
    let bytes = content.len() as u64;
    let artifact_path = artifact.to_string_lossy().into_owned();

    match runner {
        Some(runner) => stream_runner(runner, archetype, prompt, artifact_path, bytes).await,
        None => {
            if let Some(router) = crate::llm_router::CliRouter::detect() {
                let system_prompt = crate::llm::archetype_system_prompt(archetype);
                match router.call(system_prompt, prompt).await {
                    Ok(text) => {
                        tokio::fs::write(&artifact, &text)
                            .await
                            .map_err(|e| RuvosError::InternalError(format!("write artifact: {e}")))?;
                        let bytes = text.len() as u64;
                        return Ok(TaskOutcome {
                            artifact_path,
                            bytes,
                            result: format!(
                                "{archetype} agent completed via {} ({bytes} bytes)",
                                router.provider_name()
                            ),
                            success: true,
                            exit_code: Some(0),
                            stream: None,
                            content: text,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            "CliRouter failed for {archetype}: {:?} — using placeholder artifact", e
                        );
                    }
                }
            }
            Ok(TaskOutcome {
                artifact_path: artifact_path.clone(),
                bytes,
                result: format!(
                    "{} agent completed: wrote {}-byte plan to {}",
                    archetype, bytes, artifact_path
                ),
                success: true,
                exit_code: None,
                stream: None,
                content,
            })
        }
    }
}

async fn stream_runner(
    runner: &str,
    archetype: &str,
    prompt: &str,
    artifact_path: String,
    bytes: u64,
) -> Result<TaskOutcome> {
    use ruvos_stream::DriftMonitor;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    let mut child = tokio::process::Command::new(runner)
        .arg(archetype)
        .arg("--")
        .arg(prompt)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| RuvosError::InternalError(format!("runner '{}': {}", runner, e)))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RuvosError::InternalError("runner stdout unavailable".to_string()))?;
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut buf).await;
        }
        buf
    });

    let mut monitor = DriftMonitor::new(3.0);
    let mut lines = BufReader::new(stdout).lines();
    let mut collected = Vec::new();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| RuvosError::InternalError(format!("runner stream: {}", e)))?
    {
        monitor.observe(line.len() as f64);
        collected.push(line);
    }

    let status = child
        .wait()
        .await
        .map_err(|e| RuvosError::InternalError(format!("runner wait: {}", e)))?;
    let stderr_str = stderr_task.await.unwrap_or_default();

    let success = status.success();
    let mut result = collected.join("\n").trim().to_string();
    let anomalies = monitor.anomalies();
    if anomalies > 0 {
        result = format!("{result}\n[stream] {anomalies} output anomaly(ies) flagged")
            .trim()
            .to_string();
    }
    if !success {
        let stderr_str = stderr_str.trim();
        if !stderr_str.is_empty() {
            result = format!("{result}\n[stderr] {stderr_str}")
                .trim()
                .to_string();
        }
    }

    Ok(TaskOutcome {
        artifact_path,
        bytes,
        result,
        success,
        exit_code: status.code(),
        stream: Some((monitor.count(), anomalies)),
        content: String::new(),
    })
}
