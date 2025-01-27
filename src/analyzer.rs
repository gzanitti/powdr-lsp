use crate::parser::AnalyzedDoc;
use crate::span::Span;
use crate::symbol::{SemanticIndex, Symbol, SymbolDetails, SymbolKind};
use powdr_ast::analyzed::Analyzed;
use powdr_ast::asm_analysis::{AnalysisASMFile, CallableSymbol};
use powdr_ast::parsed::asm::AbsoluteSymbolPath;
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

    // TODO: Check if this could come from the parser
    // fn find_symbol_position(&mut self, symbol: &str) -> (Option<Span>, Vec<String>) {
    //     let mut log_messages = Vec::new();
    //     log_messages.push(format!("Searching for symbol: '{}'", symbol));
    //     log_messages.push(format!(
    //         "Starting search from position: {}",
    //         self.current_pos
    //     ));

    //     if let Some(pos) = self.text[self.current_pos..].find(symbol) {
    //         let start = self.current_pos + pos;
    //         let end = start + symbol.len();
    //         self.current_pos = end;

    //         // TODO: Remove log
    //         let context = self
    //             .text
    //             .get(start.saturating_sub(10)..end.saturating_add(10))
    //             .unwrap_or("");
    //         log_messages.push(format!("Found symbol at span {:?}", start..end));
    //         log_messages.push(format!("Context: '...{}...'", context));

    //         (Some(start..end), log_messages)
    //     } else {
    //         log_messages.push(format!("Symbol not found in remaining text"));
    //         (None, log_messages)
    //     }
    // }

    fn find_symbol_positions(&mut self, symbol: &str) -> (Vec<Span>, Vec<String>) {
        let mut log_messages = Vec::new();
        let mut positions = Vec::new();
        let mut search_pos = self.current_pos;

        log_messages.push(format!(
            "Searching for all occurrences of symbol: '{}'",
            symbol
        ));
        log_messages.push(format!("Starting search from position: {}", search_pos));

        while let Some(pos) = self.text[search_pos..].find(symbol) {
            let abs_start = search_pos + pos;
            let abs_end = abs_start + symbol.len();

            // Check for word boundaries using proper identifier rules
            let is_valid_start =
                abs_start == 0 || !is_identifier_char(self.text.as_bytes()[abs_start - 1] as char);
            let is_valid_end = abs_end >= self.text.len()
                || !is_identifier_char(self.text.as_bytes()[abs_end] as char);

            // Check if we're inside a comment
            let line_start = self.text[..abs_start].rfind('\n').unwrap_or(0);
            let line_content = &self.text[line_start..abs_start];
            let is_in_comment = line_content.trim_start().starts_with("//");

            if !is_in_comment && is_valid_start && is_valid_end {
                positions.push(abs_start..abs_end);
                log_messages.push(format!(
                    "Found valid occurrence at span {:?}",
                    abs_start..abs_end
                ));
            } else {
                log_messages.push(format!(
                    "Skipping occurrence at span {:?} (in_comment: {}, valid_start: {}, valid_end: {})",
                    abs_start..abs_end,
                    is_in_comment,
                    is_valid_start,
                    is_valid_end
                ));
            }

            search_pos = abs_end;
        }

        if positions.is_empty() {
            log_messages.push("No occurrences found".to_string());
        } else {
            log_messages.push(format!("Found {} occurrences", positions.len()));
        }

        (positions, log_messages)
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == ':' // TODO: Too naive
}

fn analyze_asm(asm: &AnalysisASMFile, index: &mut SemanticIndex, source_text: &str) -> Vec<String> {
    let mut tracker = PositionTracker::new(source_text);
    let mut log_messages = Vec::new();

    for (name, machine) in asm.machines() {
        let (spans, messages) = tracker
            .find_symbol_positions(&name.relative_to(&AbsoluteSymbolPath::default()).to_string());
        log_messages.extend(messages);

        let short_name = name.clone().pop().unwrap(); // TODO: Improve this
        for span in spans {
            index.add_symbol(Symbol {
                kind: SymbolKind::Machine,
                name: name.to_string(),
                span: span.clone(),
                details: SymbolDetails::Machine {
                    degree: Some(machine.degree.clone().into()),
                },
            });

            // TODO: Deduplicate this
            index.add_symbol(Symbol {
                kind: SymbolKind::Machine,
                name: short_name.to_string(),
                span,
                details: SymbolDetails::Machine {
                    degree: Some(machine.degree.clone().into()),
                },
            });
        }

        for callable in &machine.callable {
            let (spans, messages) = tracker.find_symbol_positions(&callable.name);
            log_messages.extend(messages);

            for span in spans {
                match callable.symbol {
                    CallableSymbol::Function(func) => {
                        index.add_symbol(Symbol {
                            kind: SymbolKind::Callable,
                            name: callable.name.to_string(),
                            span,
                            details: SymbolDetails::Callable {
                                inputs: func
                                    .params
                                    .inputs
                                    .iter()
                                    .map(|p| p.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                outputs: func
                                    .params
                                    .outputs
                                    .iter()
                                    .map(|p| p.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            },
                        });
                    }
                    CallableSymbol::Operation(op) => {
                        index.add_symbol(Symbol {
                            kind: SymbolKind::Callable,
                            name: callable.name.to_string(),
                            span,
                            details: SymbolDetails::Callable {
                                inputs: op
                                    .params
                                    .inputs
                                    .iter()
                                    .map(|p| p.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                outputs: op
                                    .params
                                    .outputs
                                    .iter()
                                    .map(|p| p.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            },
                        });
                    }
                }
            }
        }

        for register in &machine.registers {
            let (spans, messages) = tracker.find_symbol_positions(&register.name);
            log_messages.extend(messages);

            for span in spans {
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

    for (name, _def) in &pil.definitions {
        let (spans, messages) = tracker.find_symbol_positions(name);
        log_messages.extend(messages);

        for span in spans {
            index.add_symbol(Symbol {
                kind: SymbolKind::Definition,
                name: name.clone(),
                span,
                details: SymbolDetails::Definition,
            });
        }
    }

    for (name, _decl) in &pil.public_declarations {
        let (spans, messages) = tracker.find_symbol_positions(name);
        log_messages.extend(messages);

        for span in spans {
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
        let (spans, messages) = tracker.find_symbol_positions(name);
        log_messages.extend(messages);

        for span in spans {
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
        let (spans, messages) = tracker.find_symbol_positions(&timpl.name.to_string());
        log_messages.extend(messages);

        for span in spans {
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
