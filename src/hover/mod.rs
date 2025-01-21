use std::{collections::HashMap, iter};

use powdr::ast::{
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
}

struct Token {
    token_type: TokenType,
    values: HashMap<String, String>,
}

impl<T> HoverProvider<T> {
    pub fn new(text: String, analyzed: AnalyzedDoc<T>) -> Self {
        Self { text, analyzed }
    }

    fn get_word_at_position(&self, position: Position) -> Option<(String, usize, usize)> {
        let lines: Vec<&str> = self.text.lines().collect();
        let line = lines.get(position.line as usize)?;

        if position.character as usize >= line.len() {
            return None;
        }

        // Check if we're on a non-whitespace character
        let char_at_position = line.chars().nth(position.character as usize)?;
        if char_at_position.is_whitespace() {
            return None;
        }

        let start = line[..position.character as usize]
            .chars()
            .rev()
            .take_while(|c| !c.is_whitespace())
            .count();
        let end = line[position.character as usize..]
            .chars()
            .take_while(|c| !c.is_whitespace())
            .count();

        let initial_start = position.character as usize - start;
        let initial_end = position.character as usize + end;
        let initial_word = line[initial_start..initial_end].to_string();

        // Calculate new boundaries while cleaning up the word
        let mut word = initial_word.clone();
        let mut final_end = initial_end;

        if let Some(paren_idx) = word.find('(') {
            word = word[..paren_idx].to_string();
            final_end = initial_start + paren_idx;
        }
        if let Some(bracket_idx) = word.find('[') {
            word = word[..bracket_idx].to_string();
            final_end = initial_start + bracket_idx;
        }
        if let Some(brace_idx) = word.find('{') {
            word = word[..brace_idx].to_string();
            final_end = initial_start + brace_idx;
        }

        Some((word, initial_start, final_end))
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
            .map(|machine| Token {
                token_type: TokenType::Machine,
                values: HashMap::from_iter([
                    ("name".to_string(), word.clone()),
                    ("degree".to_string(), machine.degree.to_string()),
                ]),
            });

        let instruction_token = analyzed.clone().into_machines().find_map(|machine| {
            machine
                .1
                .callable
                .into_iter()
                .find(|function| function.name.to_string() == word)
                .map(|function| Token {
                    token_type: TokenType::Callable,
                    values: HashMap::from_iter(iter::once(("name".to_string(), word.clone()))),
                })
        });

        let register_token = analyzed.clone().into_machines().find_map(|machine| {
            machine
                .1
                .registers
                .into_iter()
                .find(|reg| reg.name.to_string() == word)
                .map(|function| Token {
                    token_type: TokenType::Register,
                    values: HashMap::from_iter(iter::once(("name".to_string(), word.clone()))),
                })
        });

        machine_token.or(instruction_token).or(register_token)
    }

    fn get_token_from_pil(&self, _analyzed: &Analyzed<T>, word: String) -> Option<Token> {
        Some(Token {
            token_type: TokenType::Callable,
            values: HashMap::from_iter([("name".to_string(), word)]),
        })
    }

    fn get_hover_content(&self, token: Token) -> String {
        match token.token_type {
            TokenType::Machine => {
                format!(
                    "### Machine Definition\n\n\
                    \n\
                    Name: {}\n\
                    Degree: {}\n\
                    Registers: \n\
                    Available Instructions: \n\
                    \n\n\
                    Small explanation of what a machine is.",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                    token
                        .values
                        .get("degree")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
            TokenType::Callable => {
                format!(
                    "### Instruction\n\n\
                    \n\
                    Name: {}\n\
                    Parameters: \n\
                    Selector Columns: \n\
                    \n\n\
                    Small explanation of what an instruction is.",
                    token
                        .values
                        .get("name")
                        .map_or("".to_string(), |v| v.to_string()),
                )
            }
            TokenType::Register => {
                format!(
                    "### Register\n\n\
                    \n\
                    Name: {}\n\
                    \n\n\
                    Small explanation of what a register is.",
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
