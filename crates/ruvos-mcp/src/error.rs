// crates/ruvos-mcp/src/error.rs

#[derive(Debug)]
pub enum RuvosError {
    // JSON-RPC protocol errors
    ParseError(String),     // -32700
    InvalidRequest(String), // -32600
    MethodNotFound,         // -32601
    InvalidParams(String),  // -32602
    InternalError(String),  // -32603

    // Handler errors
    HandlerError(String),
    ValidationError(String),
}

impl RuvosError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            RuvosError::ParseError(_) => -32700,
            RuvosError::InvalidRequest(_) => -32600,
            RuvosError::MethodNotFound => -32601,
            RuvosError::InvalidParams(_) => -32602,
            RuvosError::InternalError(_)
            | RuvosError::HandlerError(_)
            | RuvosError::ValidationError(_) => -32000,
        }
    }

    pub fn message(&self) -> String {
        match self {
            RuvosError::ParseError(msg) => format!("Parse error: {}", msg),
            RuvosError::InvalidRequest(msg) => format!("Invalid Request: {}", msg),
            RuvosError::MethodNotFound => "Method not found".to_string(),
            RuvosError::InvalidParams(msg) => format!("Invalid params: {}", msg),
            RuvosError::InternalError(msg) => format!("Internal error: {}", msg),
            RuvosError::HandlerError(msg) => format!("Handler error: {}", msg),
            RuvosError::ValidationError(msg) => format!("Validation error: {}", msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, RuvosError>;
