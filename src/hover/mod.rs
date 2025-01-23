use std::collections::HashMap;

use powdr_ast::{
    analyzed::Analyzed, asm_analysis::AnalysisASMFile, parsed::asm::parse_absolute_path,
};
use tower_lsp::lsp_types::*;

use crate::parser::AnalyzedDoc;

pub struct HoverProvider<T> {
    text: String,
    analyzed: AnalyzedDoc<T>,
}

enum TokenType {
    Machine,
    Callable,
    Register,
    Definition, //TODO: Specialize this for different types of definitions
    Public,
    Intermediate,
    TraitImpl,
}

struct Token {
    token_type: TokenType,
    values: HashMap<String, String>,
}

impl<T> HoverProvider<T> {
    pub fn new(text: String, analyzed: AnalyzedDoc<T>) -> Self {
        Self { text, analyzed }
    }

    pub fn get_word_at_position(&self, position: Position) -> Option<(String, usize, usize)> {
        let lines: Vec<&str> = self.text.lines().collect();
        let line = lines.get(position.line as usize)?;

        if position.character as usize >= line.len() {
            return None;
        }

        // Check if we're on a non-whitespace character
        let char_at_position = line.chars().nth(position.character as usize)?;
        if !is_identifier_char(char_at_position) {
            return None;
        }

        let start = line[..position.character as usize]
            .chars()
            .rev()
            .take_while(|c| is_identifier_char(*c))
            .count();
        let end = line[position.character as usize..]
            .chars()
            .take_while(|c| is_identifier_char(*c))
            .count();

        let initial_start = position.character as usize - start;
        let initial_end = position.character as usize + end;
        let word = line[initial_start..initial_end].to_string();

        Some((word, initial_start, initial_end))
    }

    fn get_token_at_position(&self, position: Position) -> Option<Token> {
        let (word, _word_start, _word_end) = self.get_word_at_position(position)?;

        match &self.analyzed {
            AnalyzedDoc::ASM(asm) => self.get_token_from_asm(asm, word),
            AnalyzedDoc::PIL(pil) => self.get_token_from_pil(pil, word),
        }
    }

    fn get_token_from_asm(&self, analyzed: &AnalysisASMFile, word: String) -> Option<Token> {
        let machine_token = analyzed
            .get_machine(&parse_absolute_path(&format!("::{}", word)))
            .map(|machine| {
                let degree_values = match (machine.degree.min.clone(), machine.degree.max.clone()) {
                    (Some(min), Some(max)) => {
                        if min == max {
                            vec![("degree".to_string(), min.to_string())]
                        } else {
                            vec![
                                ("degree_min".to_string(), min.to_string()),
                                ("degree_max".to_string(), max.to_string()),
                            ]
                        }
                    }
                    (Some(val), None) | (None, Some(val)) => {
                        vec![("degree".to_string(), val.to_string())]
                    }
                    (None, None) => vec![],
                };

                Token {
                    token_type: TokenType::Machine,
                    values: HashMap::from_iter(
                        [("name".to_string(), word.clone())]
                            .into_iter()
                            .chain(degree_values),
                    ),
                }
            });

        // TODO: avoid clone
        let instruction_token = analyzed.clone().into_machines().find_map(|machine| {
            machine
                .1
                .callable
                .into_iter()
                .find(|function| function.name.to_string() == word)
                .map(|function| Token {
                    token_type: TokenType::Callable,
                    values: HashMap::from_iter([
                        ("name".to_string(), word.clone()),
                        ("symbol".to_string(), format!("{:?}", function.symbol)),
                    ]),
                })
        });

        //TODO: Avoid clone
        let register_token = analyzed.clone().into_machines().find_map(|machine| {
            machine
                .1
                .registers
                .into_iter()
                .find(|reg| reg.name.to_string() == word)
                .map(|reg| Token {
                    token_type: TokenType::Register,
                    values: HashMap::from_iter([
                        ("name".to_string(), word.clone()),
                        ("type".to_string(), reg.ty.to_string()),
                    ]),
                })
        });

        machine_token.or(instruction_token).or(register_token)
    }

    fn get_token_from_pil(&self, analyzed: &Analyzed<T>, word: String) -> Option<Token> {
        //TODO: Find a better way to do this.
        let definition = analyzed.definitions.get(&word).map(|_def| Token {
            token_type: TokenType::Definition,
            values: HashMap::from_iter([("name".to_string(), word.clone())]),
        });

        let public = analyzed.public_declarations.get(&word).map(|_def| Token {
            token_type: TokenType::Public,
            values: HashMap::from_iter([("name".to_string(), word.clone())]),
        });

        let intermediate = analyzed.intermediate_columns.get(&word).map(|_def| Token {
            token_type: TokenType::Intermediate,
            values: HashMap::from_iter([("name".to_string(), word.clone())]),
        });

        let trait_impl = analyzed
            .trait_impls
            .iter()
            .find(|timpl| timpl.name.to_string() == word) //TODO: Resolve String to SymbolPath
            .map(|_timpl| Token {
                token_type: TokenType::TraitImpl,
                values: HashMap::from_iter([("name".to_string(), word.clone())]),
            });

        definition.or(public).or(intermediate).or(trait_impl) //TODO: Too naive
    }

    fn get_hover_content(&self, token: Token) -> String {
        match token.token_type {
            TokenType::Machine => {
                let degree_text = if let Some(degree) = token.values.get("degree") {
                    degree.to_string()
                } else {
                    match (
                        token.values.get("degree_min"),
                        token.values.get("degree_max"),
                    ) {
                        (Some(min), Some(max)) => format!(" Min:{}, Max:{}", min, max),
                        _ => "".to_string(),
                    }
                };

                format!(
                    "### Machine\n\n\
                    \n\
                    Name: {}\n\n\
                    {}\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                    if !degree_text.is_empty() {
                        format!("Degree: {}\n", degree_text)
                    } else {
                        String::new()
                    }
                )
            }
            TokenType::Callable => {
                format!(
                    "### Instruction\n\n\
                    \n\
                    Name: {}\n\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
            TokenType::Register => {
                let type_text = token
                    .values
                    .get("type")
                    .map_or(String::new(), |v| format!("Type: {}\n\n", v));

                format!(
                    "### Register\n\n\
                    \n\
                    Name: {}\n\n\
                    {}\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                    type_text
                )
            }
            TokenType::Definition => {
                format!(
                    "### Definition\n\n\
                    \n\
                    Name: {}\n\n\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
            TokenType::Public => {
                format!(
                    "### Public\n\n\
                    \n\
                    Name: {}\n\n\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
            TokenType::Intermediate => {
                format!(
                    "### Intermediate\n\n\
                    \n\
                    Name: {}\n\n\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
            TokenType::TraitImpl => {
                format!(
                    "### Trait Implementation\n\n\
                    \n\
                    Name: {}\n\n\
                    \n\n",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
        }
    }
    pub fn get_hover(&self, position: Position) -> Option<Hover> {
        let token = self.get_token_at_position(position)?;
        let content = self.get_hover_content(token);

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),

            range: Some(Range::new(position, position)),
        })
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == ':' // TODO: Too naive
}
