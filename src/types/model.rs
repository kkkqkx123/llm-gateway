use serde::{Deserialize, Serialize};

/// modeling capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelCapabilities {
    /// Support Thinking
    pub thinking: bool,
    /// Streaming response support
    pub streaming: bool,
    /// Support for function calls
    pub function_calling: bool,
    /// Multi-modal support (image, audio, etc.)
    pub multimodal: bool,
    /// Maximum context length (number of tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_length: Option<usize>,
    /// Supported Input Types
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub input_types: Vec<String>,
    /// Supported Output Types
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub output_types: Vec<String>,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            thinking: false,
            streaming: true,
            function_calling: false,
            multimodal: false,
            max_context_length: None,
            input_types: vec!["text".to_string()],
            output_types: vec!["text".to_string()],
        }
    }
}

impl ModelCapabilities {
    /// Creation of basic competencies
    pub fn basic() -> Self {
        Self::default()
    }

    /// Creating models that support Thinking
    pub fn with_thinking() -> Self {
        Self {
            thinking: true,
            streaming: true,
            function_calling: false,
            multimodal: false,
            max_context_length: Some(200000),
            input_types: vec!["text".to_string()],
            output_types: vec!["text".to_string()],
        }
    }

    /// Creating models that support multimodality
    pub fn with_multimodal() -> Self {
        Self {
            thinking: false,
            streaming: true,
            function_calling: false,
            multimodal: true,
            max_context_length: Some(200000),
            input_types: vec!["text".to_string(), "image".to_string()],
            output_types: vec!["text".to_string()],
        }
    }

    /// Creating models that support function calls
    pub fn with_function_calling() -> Self {
        Self {
            thinking: false,
            streaming: true,
            function_calling: true,
            multimodal: false,
            max_context_length: Some(128000),
            input_types: vec!["text".to_string()],
            output_types: vec!["text".to_string()],
        }
    }

    /// Check if specific capabilities are supported
    pub fn supports(&self, capability: &str) -> bool {
        match capability {
            "thinking" => self.thinking,
            "streaming" => self.streaming,
            "function_calling" => self.function_calling,
            "multimodal" => self.multimodal,
            _ => false,
        }
    }
}

/// Model Pricing Information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Enter the price (per 1K token)
    pub input_price: f64,
    /// Output price (per 1K token)
    pub output_price: Option<f64>,
    /// currency unit
    pub currency: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(!caps.thinking);
        assert!(caps.streaming);
        assert!(!caps.function_calling);
        assert!(!caps.multimodal);
        assert!(caps.max_context_length.is_none());
        assert_eq!(caps.input_types, vec!["text"]);
        assert_eq!(caps.output_types, vec!["text"]);
    }

    #[test]
    fn test_model_capabilities_with_thinking() {
        let caps = ModelCapabilities::with_thinking();
        assert!(caps.thinking);
        assert!(caps.streaming);
        assert!(!caps.function_calling);
        assert!(!caps.multimodal);
        assert_eq!(caps.max_context_length, Some(200000));
    }

    #[test]
    fn test_model_capabilities_with_multimodal() {
        let caps = ModelCapabilities::with_multimodal();
        assert!(!caps.thinking);
        assert!(caps.streaming);
        assert!(!caps.function_calling);
        assert!(caps.multimodal);
        assert_eq!(caps.input_types, vec!["text", "image"]);
    }

    #[test]
    fn test_model_capabilities_supports() {
        let caps = ModelCapabilities::with_thinking();
        assert!(caps.supports("thinking"));
        assert!(caps.supports("streaming"));
        assert!(!caps.supports("function_calling"));
        assert!(!caps.supports("multimodal"));
        assert!(!caps.supports("unknown"));
    }
}
