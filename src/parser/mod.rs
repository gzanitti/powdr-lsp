use std::path::PathBuf;

use powdr_ast::analyzed::Analyzed;
use powdr_ast::asm_analysis::AnalysisASMFile;
use powdr_parser_util::Error as PowdrError;

use powdr_number::FieldElement;
use powdr_parser;
use powdr_pil_analyzer;
use tower_lsp::lsp_types::*;

pub struct ParseResult<T> {
    pub diagnostics: Vec<Diagnostic>,
    pub analyzed: AnalyzedDoc<T>,
}

pub struct Error {
    pub message: String,
    pub source_pos: SourcePos,
}

impl Error {
    pub fn new(message: String, source_pos: SourcePos) -> Self {
        Self {
            message,
            source_pos,
        }
    }

    pub fn message(&self) -> &String {
        &self.message
    }

    pub fn source_pos(&self) -> &SourcePos {
        &self.source_pos
    }
}

impl From<PowdrError> for Error {
    fn from(e: PowdrError) -> Self {
        Error {
            message: e.to_string(),
            source_pos: SourcePos::new(e.source_ref().start, e.source_ref().end),
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.message
    }
}

pub struct SourcePos {
    pub start: usize,
    pub end: usize,
}

impl SourcePos {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn unknown() -> Self {
        Self { start: 0, end: 0 }
    }
}

#[derive(Debug, Clone)]
pub enum AnalyzedDoc<T> {
    ASM(AnalysisASMFile),
    PIL(Analyzed<T>),
}

pub fn parse<T: FieldElement>(content: &str, uri: &Url) -> ParseResult<T> {
    let result = if uri.path().ends_with(".asm") {
        match parse_asm(uri.path(), content) {
            Ok(asm) => Ok(AnalyzedDoc::ASM(asm)),
            Err(e) => Err(e),
        }
    } else {
        match parse_pil::<T>(content) {
            Ok(pil) => Ok(AnalyzedDoc::PIL(pil)),
            Err(e) => Err(e.into()),
        }
    };

    match result {
        Ok(analyzed) => ParseResult {
            diagnostics: vec![],
            analyzed,
        },
        Err(err) => {
            let diagnostics = err
                .iter()
                .map(|e| Diagnostic {
                    range: Range {
                        start: convert_position(e.source_pos().start, content),
                        end: convert_position(e.source_pos().end, content),
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: e.message().to_string(),
                    source: Some("powdr".to_string()),
                    ..Default::default()
                })
                .collect();

            ParseResult {
                diagnostics,
                analyzed: AnalyzedDoc::ASM(AnalysisASMFile::default()), // Default in case of error
            }
        }
    }
}
fn parse_asm(path: &str, content: &str) -> Result<AnalysisASMFile, Vec<Error>> {
    let parsed_asm = match powdr_parser::parse_asm(Some(path), content) {
        Ok(asm) => asm,
        Err(e) => return Err(vec![e.into()]),
    };

    let resolved =
        powdr_importer::load_dependencies_and_resolve(Some(PathBuf::from(path)), parsed_asm)
            .map_err(|e| vec![e.into()])?;

    powdr_analysis::analyze(resolved).map_err(|strings| {
        strings
            .into_iter()
            .map(|message| Error {
                message,
                source_pos: SourcePos::unknown(),
            })
            .collect()
    })
}

fn parse_pil<T: FieldElement>(content: &str) -> Result<Analyzed<T>, Vec<Error>> {
    match powdr_pil_analyzer::analyze_string::<T>(content) {
        Ok(pil) => Ok(pil),
        Err(e) => Err(e.into_iter().map(|err| err.into()).collect()),
    }
}
fn convert_position(offset: usize, content: &str) -> Position {
    let content_until_offset = &content[..offset];
    let line = content_until_offset.chars().filter(|&c| c == '\n').count() as u32;

    let last_newline = content_until_offset
        .rfind('\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);

    let column = (offset - last_newline) as u32;

    Position::new(line, column)
}
