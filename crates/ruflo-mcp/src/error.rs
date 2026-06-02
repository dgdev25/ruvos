// crates/ruflo-mcp/src/error.rs

#[derive(Debug)]
pub enum RufloError {
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

impl RufloError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            RufloError::ParseError(_) => -32700,
            RufloError::InvalidRequest(_) => -32600,
            RufloError::MethodNotFound => -32601,
            RufloError::InvalidParams(_) => -32602,
            RufloError::InternalError(_)
            | RufloError::HandlerError(_)
            | RufloError::ValidationError(_) => -32000,
        }
    }

    pub fn message(&self) -> String {
        match self {
            RufloError::ParseError(msg) => format!("Parse error: {}", msg),
            RufloError::InvalidRequest(msg) => format!("Invalid Request: {}", msg),
            RufloError::MethodNotFound => "Method not found".to_string(),
            RufloError::InvalidParams(msg) => format!("Invalid params: {}", msg),
            RufloError::InternalError(msg) => format!("Internal error: {}", msg),
            RufloError::HandlerError(msg) => format!("Handler error: {}", msg),
            RufloError::ValidationError(msg) => format!("Validation error: {}", msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, RufloError>;
