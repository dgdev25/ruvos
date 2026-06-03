// crates/ruvos-mcp/src/error.rs

#[derive(Debug)]
pub enum rUvOSError {
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

impl rUvOSError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            rUvOSError::ParseError(_) => -32700,
            rUvOSError::InvalidRequest(_) => -32600,
            rUvOSError::MethodNotFound => -32601,
            rUvOSError::InvalidParams(_) => -32602,
            rUvOSError::InternalError(_)
            | rUvOSError::HandlerError(_)
            | rUvOSError::ValidationError(_) => -32000,
        }
    }

    pub fn message(&self) -> String {
        match self {
            rUvOSError::ParseError(msg) => format!("Parse error: {}", msg),
            rUvOSError::InvalidRequest(msg) => format!("Invalid Request: {}", msg),
            rUvOSError::MethodNotFound => "Method not found".to_string(),
            rUvOSError::InvalidParams(msg) => format!("Invalid params: {}", msg),
            rUvOSError::InternalError(msg) => format!("Internal error: {}", msg),
            rUvOSError::HandlerError(msg) => format!("Handler error: {}", msg),
            rUvOSError::ValidationError(msg) => format!("Validation error: {}", msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, rUvOSError>;
