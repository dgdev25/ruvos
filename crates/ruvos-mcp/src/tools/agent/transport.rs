use ruv_swarm_transport::{
    in_process::{InProcessRegistry, InProcessTransport},
    protocol::{Message, MessageType},
    TransportConfig,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    pub(crate) static ref TRANSPORT_REGISTRY: Arc<InProcessRegistry> = InProcessRegistry::new();
    static ref AGENT_TRANSPORTS: Mutex<HashMap<String, InProcessTransport>> =
        Mutex::new(HashMap::new());
}

fn transport_config() -> TransportConfig {
    TransportConfig::default()
}

pub(super) async fn register_agent_transport(agent_id: &str) {
    match InProcessTransport::new(
        agent_id.to_string(),
        transport_config(),
        Arc::clone(&TRANSPORT_REGISTRY),
    )
    .await
    {
        Ok(transport) => {
            if let Ok(mut map) = AGENT_TRANSPORTS.lock() {
                map.insert(agent_id.to_string(), transport);
            }
        }
        Err(e) => {
            tracing::warn!("transport register failed for {}: {}", agent_id, e);
        }
    }
}

pub(super) async fn transport_send(agent_id: &str, content: &str) {
    let msg = Message::new(
        "system".to_string(),
        MessageType::Event {
            name: "agent.message".to_string(),
            data: serde_json::json!({ "content": content }),
        },
    );
    if let Err(e) = TRANSPORT_REGISTRY.send("system", agent_id, msg).await {
        tracing::debug!("transport send to {}: {} (non-fatal)", agent_id, e);
    }
}
