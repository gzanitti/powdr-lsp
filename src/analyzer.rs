use crate::parser::AnalyzedDoc;
use crate::span::Span;
use crate::symbol::{SemanticIndex, Symbol, SymbolDetails, SymbolKind};
use powdr_ast::analyzed::Analyzed;
use powdr_ast::asm_analysis::AnalysisASMFile;
use tower_lsp::Client;
use tower_lsp::lsp_types::MessageType;

pub fn build_semantic_index<T>(
    doc: &AnalyzedDoc<T>,
    source_text: &str,
) -> (SemanticIndex, Vec<String>) {
    let mut index = SemanticIndex::new();

    let errors = match doc {
        AnalyzedDoc::ASM(asm) => analyze_asm(asm, &mut index, source_text),
        AnalyzedDoc::PIL(pil) => analyze_pil(pil, &mut index, source_text),
    };

    (index, errors)
}
struct PositionTracker<'a> {
    text: &'a str,
    current_pos: usize,
}

impl<'a> PositionTracker<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            current_pos: 0,
        }
    }

    // TODO: Check in this could come from the parser
    fn find_symbol_position(&mut self, symbol: &str) -> (Option<Span>, Vec<String>) {
        let mut log_messages = Vec::new();
        log_messages.push(format!("Searching for symbol: '{}'", symbol));
        log_messages.push(format!(
            "Starting search from position: {}",
            self.current_pos
        ));

        if let Some(pos) = self.text[self.current_pos..].find(symbol) {
            let start = self.current_pos + pos;
            let end = start + symbol.len();
            self.current_pos = end;

            // Log the context where we found the symbol
            let context = self
                .text
                .get(start.saturating_sub(10)..end.saturating_add(10))
                .unwrap_or("");
            log_messages.push(format!("Found symbol at span {:?}", start..end));
            log_messages.push(format!("Context: '...{}...'", context));

            (Some(start..end), log_messages)
        } else {
            log_messages.push(format!("Symbol not found in remaining text"));
            (None, log_messages)
        }
    }
}

fn analyze_asm(asm: &AnalysisASMFile, index: &mut SemanticIndex, source_text: &str) -> Vec<String> {
    //let mut tracker = PositionTracker::new(source_text);
    let mut log_messages = Vec::new();

    for (name, machine) in asm.machines() {
        let mut tracker = PositionTracker::new(source_text);
        let (span, messages) = tracker.find_symbol_position(&name.to_string());
        log_messages.extend(messages);

        if let Some(span) = span {
            index.add_symbol(Symbol {
                kind: SymbolKind::Machine,
                name: name.to_string(),
                span,
                details: SymbolDetails::Machine {
                    degree: Some(machine.degree.clone().into()),
                },
            });
        }

        for callable in &machine.callable {
            let (span, messages) = tracker.find_symbol_position(&callable.name);
            log_messages.extend(messages);

            if let Some(span) = span {
                index.add_symbol(Symbol {
                    kind: SymbolKind::Callable,
                    name: callable.name.to_string(),
                    span,
                    details: SymbolDetails::Callable {
                        symbol: format!("{:?}", callable.symbol),
                    },
                });
            }
        }

        for register in &machine.registers {
            let (span, messages) = tracker.find_symbol_position(&register.name);
            log_messages.extend(messages);

            if let Some(span) = span {
                index.add_symbol(Symbol {
                    kind: SymbolKind::Register,
                    name: register.name.to_string(),
                    span,
                    details: SymbolDetails::Register {
                        type_info: register.ty.to_string(),
                    },
                });
            }
        }
    }

    log_messages
}
fn analyze_pil<T>(pil: &Analyzed<T>, index: &mut SemanticIndex, source_text: &str) -> Vec<String> {
    let mut tracker = PositionTracker::new(source_text);
    let mut log_messages = Vec::new();

    // Add definition symbols
    for (name, _def) in &pil.definitions {
        let (span, messages) = tracker.find_symbol_position(name);
        log_messages.extend(messages);

        if let Some(span) = span {
            index.add_symbol(Symbol {
                kind: SymbolKind::Definition,
                name: name.clone(),
                span,
                details: SymbolDetails::Definition,
            });
        }
    }

    // Add public symbols
    for (name, _decl) in &pil.public_declarations {
        let (span, messages) = tracker.find_symbol_position(name);
        log_messages.extend(messages);

        if let Some(span) = span {
            index.add_symbol(Symbol {
                kind: SymbolKind::Public,
                name: name.clone(),
                span,
                details: SymbolDetails::Public,
            });
        }
    }

    // Add intermediate symbols
    for (name, _col) in &pil.intermediate_columns {
        let (span, messages) = tracker.find_symbol_position(name);
        log_messages.extend(messages);

        if let Some(span) = span {
            index.add_symbol(Symbol {
                kind: SymbolKind::Intermediate,
                name: name.clone(),
                span,
                details: SymbolDetails::Intermediate,
            });
        }
    }

    // Add trait implementation symbols
    for timpl in &pil.trait_impls {
        let (span, messages) = tracker.find_symbol_position(&timpl.name.to_string());
        log_messages.extend(messages);

        if let Some(span) = span {
            index.add_symbol(Symbol {
                kind: SymbolKind::TraitImpl,
                name: timpl.name.to_string(),
                span,
                details: SymbolDetails::TraitImpl,
            });
        }
    }

    log_messages
}
