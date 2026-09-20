pub mod claude;
pub mod gemini;
pub mod openai;

// 重新导出所有内容
pub use claude::*;
pub use gemini::*;
pub use openai::*;

// 向后兼容的模块别名
pub mod openai_to_claude {
    pub use crate::converters::openai::to_claude::*;
}
pub mod openai_to_gemini {
    pub use crate::converters::openai::to_gemini::*;
}
pub mod openai_to_openai_response {
    pub use crate::converters::openai::to_openai_response::*;
}
pub mod openai_response_to_openai {
    pub use crate::converters::openai::from_openai_response::*;
}
pub mod claude_to_openai {
    pub use crate::converters::claude::to_openai::*;
}
pub mod claude_to_gemini {
    pub use crate::converters::claude::to_gemini::*;
}
pub mod gemini_to_openai {
    pub use crate::converters::gemini::to_openai::*;
}
pub mod gemini_to_claude {
    pub use crate::converters::gemini::to_claude::*;
}
