use crate::span::Span;
use rust_lapper::{Interval, Lapper};
use std::collections::HashMap;

pub type SymbolId = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Machine,
    Callable,
    Register,
    Definition,
    Public,
    Intermediate,
    TraitImpl,
}
#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub span: Span,
    pub name: String,
    pub details: SymbolDetails,
}

#[derive(Debug, Clone)]
pub enum SymbolDetails {
    Machine { degree: Option<DegreeInfo> },
    Register { type_info: String },
    Callable { symbol: String },
    Definition,
    Public,
    Intermediate,
    TraitImpl,
}

#[derive(Debug, Clone)]
pub struct DegreeInfo {
    pub min: Option<u64>,
    pub max: Option<u64>,
}

impl From<powdr_ast::asm_analysis::MachineDegree> for DegreeInfo {
    fn from(degree: powdr_ast::asm_analysis::MachineDegree) -> Self {
        DegreeInfo {
            min: Some(8 as u64), //TODO evaluate expr
            max: Some(10 as u64),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticIndex {
    pub symbols: HashMap<SymbolId, Symbol>,
    pub range_index: Lapper<usize, SymbolId>,
}

impl SemanticIndex {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            range_index: Lapper::new(vec![]),
        }
    }

    pub fn add_symbol(&mut self, symbol: Symbol) -> SymbolId {
        let id = self.symbols.len() as SymbolId;
        self.range_index.insert(Interval {
            start: symbol.span.start,
            stop: symbol.span.end,
            val: id,
        });
        self.symbols.insert(id, symbol);
        id
    }

    pub fn find_symbol_at_position(&self, offset: usize) -> Option<&Symbol> {
        self.range_index
            .find(offset, offset + 1)
            .next()
            .and_then(|interval| self.symbols.get(&interval.val))
    }

    pub fn get_all_ranges(&self) -> Vec<(Span, &Symbol)> {
        self.range_index
            .iter()
            .filter_map(|interval| {
                self.symbols
                    .get(&interval.val)
                    .map(|symbol| (interval.start..interval.stop, symbol))
            })
            .collect()
    }
}
