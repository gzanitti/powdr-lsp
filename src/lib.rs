pub mod analyzer;
pub mod hover;
pub mod parser;
pub mod span;
pub mod symbol;

pub use analyzer::build_semantic_index;
pub use hover::HoverProvider;
pub use parser::{AnalyzedDoc, ParseResult, parse};
pub use span::Span;
pub use symbol::{SemanticIndex, Symbol, SymbolDetails, SymbolId, SymbolKind};
