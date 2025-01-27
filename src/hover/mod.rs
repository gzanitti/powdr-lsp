use std::collections::HashMap;

use crate::parser::AnalyzedDoc;
use crate::symbol::{Symbol, SymbolDetails, SymbolKind};
use powdr_ast::{
    analyzed::Analyzed, asm_analysis::AnalysisASMFile, parsed::asm::parse_absolute_path,
};
use tower_lsp::lsp_types::*;

pub struct HoverProvider<T> {
    text: String,
    analyzed: AnalyzedDoc<T>,
    semantic_index: crate::symbol::SemanticIndex,
}

impl<T> HoverProvider<T> {
    pub fn new(
        text: String,
        analyzed: AnalyzedDoc<T>,
        semantic_index: crate::symbol::SemanticIndex,
    ) -> Self {
        Self {
            text,
            analyzed,
            semantic_index,
        }
    }

    pub fn get_hover(&self, position: Position) -> (Option<Hover>, Vec<String>) {
        let mut log_messages = Vec::new();

        let offset = match self.position_to_offset(position) {
            Some(off) => {
                let context = self
                    .text
                    .get(off.saturating_sub(10)..off.saturating_add(10))
                    .unwrap_or("")
                    .to_string();
                log_messages.push(format!(
                    "Converting position Line:{}, Char:{} to offset {}. Text context: '{}'",
                    position.line, position.character, off, context
                ));

                //log_messages.push(format!(
                //    "Available symbol ranges: {:?}",
                //    self.semantic_index.get_all_ranges()
                //));

                off
            }
            None => {
                log_messages.push("Failed to convert position to offset".to_string());
                return (None, log_messages);
            }
        };

        let symbol = match self.semantic_index.find_symbol_at_position(offset) {
            Some(sym) => {
                log_messages.push(format!("Found symbol at offset {}: {:?}", offset, sym));
                sym
            }
            None => {
                log_messages.push(format!("No symbol found at offset {}", offset));
                return (None, log_messages);
            }
        };

        let content = self.get_hover_content(symbol);
        log_messages.push(format!(
            "Generated hover content: {} for symbol {:?}",
            content, symbol
        ));

        let hover = Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: Some(Range::new(position, position)),
        });

        (hover, log_messages)
    }

    fn position_to_offset(&self, position: Position) -> Option<usize> {
        let lines: Vec<&str> = self.text.lines().collect();
        let line = lines.get(position.line as usize)?;

        let mut offset = self
            .text
            .lines()
            .take(position.line as usize)
            .map(|line| line.len() + 1)
            .sum::<usize>();

        offset += position.character as usize;
        Some(offset)
    }

    fn get_hover_content(&self, symbol: &Symbol) -> String {
        match (&symbol.kind, &symbol.details) {
            (SymbolKind::Machine, SymbolDetails::Machine { degree }) => {
                let degree_text = match degree {
                    Some(info) => match (info.min, info.max) {
                        (Some(min), Some(max)) if min == max => format!("Degree: {}", min),
                        (Some(min), Some(max)) => format!("Degree: Min:{}, Max:{}", min, max),
                        (Some(val), None) | (None, Some(val)) => format!("Degree: {}", val),
                        (None, None) => String::new(),
                    },
                    None => String::new(),
                };

                format!(
                    "### Machine\n\n\
                    Name: {}\n\
                    {}\n",
                    symbol.name, degree_text
                )
            }
            (SymbolKind::Register, SymbolDetails::Register { type_info }) => {
                if type_info.is_empty() {
                    format!(
                        "### Register\n\n\
                        Name: {}\n",
                        symbol.name
                    )
                } else {
                    format!(
                        "### Register\n\n\
                        Name: {}\n\
                        Type: {}\n",
                        symbol.name, type_info
                    )
                }
            }
            (SymbolKind::Callable, SymbolDetails::Callable { inputs, outputs }) => {
                format!(
                    "### Instruction\n\n\
                    Name: {}\n\n\
                    Inputs: {}\n\n\
                    Outputs: {}\n",
                    symbol.name, inputs, outputs
                )
            }
            (SymbolKind::Definition, SymbolDetails::Definition) => {
                format!(
                    "### Definition\n\n\
                    Name: {}\n",
                    symbol.name
                )
            }
            (SymbolKind::Public, SymbolDetails::Public) => {
                format!(
                    "### Public\n\n\
                    Name: {}\n",
                    symbol.name
                )
            }
            (SymbolKind::Intermediate, SymbolDetails::Intermediate) => {
                format!(
                    "### Intermediate\n\n\
                    Name: {}\n",
                    symbol.name
                )
            }
            (SymbolKind::TraitImpl, SymbolDetails::TraitImpl) => {
                format!(
                    "### Trait Implementation\n\n\
                    Name: {}\n",
                    symbol.name
                )
            }
            _ => format!("### Symbol\n\nName: {}\n", symbol.name),
        }
    }
}
