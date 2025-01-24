mod analyzer;
mod hover;
mod parser;
mod span;
mod symbol;

use powdr_number::{FieldElement, GoldilocksField};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::analyzer::build_semantic_index;
use crate::hover::HoverProvider;
use crate::parser::{AnalyzedDoc, ParseResult};
use crate::symbol::{SemanticIndex, Symbol, SymbolDetails, SymbolId, SymbolKind};

#[derive(Debug)]
struct Backend<T: FieldElement> {
    client: Client,
    project_cache: RwLock<ProjectCache<T>>,
}

#[derive(Debug, Clone)]
struct ParsedDocument<T> {
    analyzed: AnalyzedDoc<T>,
    text: String,
    version: i32,
    semantic_index: SemanticIndex,
}

#[derive(Debug)]
struct ProjectCache<T> {
    documents: HashMap<Url, ParsedDocument<T>>,
    symbol_locations: HashMap<String, Vec<(Url, SymbolKind)>>,
}

impl<T> ProjectCache<T> {
    fn new() -> Self {
        Self {
            documents: HashMap::new(),
            symbol_locations: HashMap::new(),
        }
    }

    fn update_document(&mut self, uri: Url, doc: ParsedDocument<T>) {
        self.remove_document_symbols(&uri);

        // Actualizar symbol_locations basado en el nuevo semantic_index
        for (_, symbol) in doc.semantic_index.symbols.iter() {
            self.symbol_locations
                .entry(symbol.name.clone())
                .or_default()
                .push((uri.clone(), symbol.kind.clone()));
        }

        self.documents.insert(uri, doc);
    }

    fn remove_document_symbols(&mut self, uri: &Url) {
        for locations in self.symbol_locations.values_mut() {
            locations.retain(|(doc_uri, _)| doc_uri != uri);
        }

        self.symbol_locations
            .retain(|_, locations| !locations.is_empty());
    }

    fn get_symbol_locations(&self, name: &str) -> Vec<(Url, SymbolKind)> {
        self.symbol_locations.get(name).cloned().unwrap_or_default()
    }
}
impl<T: FieldElement> Backend<T> {
    async fn scan_workspace_folder(&self, folder_uri: Url) -> Result<()> {
        let folder_path = folder_uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: "Invalid folder URI".to_string().into(),
                data: None,
            })?;

        self.scan_directory(&folder_path).await
    }

    async fn scan_directory(&self, dir: &Path) -> Result<()> {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Scanning directory: {}", dir.display()),
            )
            .await;
        for entry in fs::read_dir(dir).map_err(|e| tower_lsp::jsonrpc::Error {
            code: tower_lsp::jsonrpc::ErrorCode::InternalError,
            message: e.to_string().into(),
            data: None,
        })? {
            let entry = entry.map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: e.to_string().into(),
                data: None,
            })?;
            let path = entry.path();

            if path.is_dir() {
                Box::pin(self.scan_directory(&path)).await?;
            } else if let Some(extension) = path.extension() {
                if extension == "pil" || extension == "asm" {
                    let content =
                        fs::read_to_string(&path).map_err(|e| tower_lsp::jsonrpc::Error {
                            code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                            message: e.to_string().into(),
                            data: None,
                        })?;
                    let uri =
                        Url::from_file_path(&path).map_err(|_| tower_lsp::jsonrpc::Error {
                            code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                            message: "Invalid file path".to_string().into(),
                            data: None,
                        })?;

                    let result = crate::parser::parse::<T>(&content, &uri);
                    let (semantic_index, log_messages) =
                        crate::analyzer::build_semantic_index(&result.analyzed, &content);

                    for message in log_messages {
                        self.client.log_message(MessageType::INFO, message).await;
                    }

                    let doc = ParsedDocument {
                        analyzed: result.analyzed,
                        text: content,
                        version: 0,
                        semantic_index,
                    };

                    self.project_cache
                        .write()
                        .unwrap()
                        .update_document(uri, doc);
                }
            }
        }
        Ok(())
    }
}
#[tower_lsp::async_trait]
impl<T: FieldElement> LanguageServer for Backend<T> {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, "Starting workspace initialization...")
            .await;

        if let Some(workspace_folders) = params.workspace_folders {
            for folder in workspace_folders {
                self.scan_workspace_folder(folder.uri).await?;
            }
        }

        self.client
            .log_message(MessageType::INFO, "Workspace initialization completed")
            .await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Powdr LSP initialized!")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        let result = crate::parser::parse::<T>(&text, &uri);
        let (semantic_index, log_messages) =
            crate::analyzer::build_semantic_index(&result.analyzed, &text);

        for message in log_messages {
            self.client.log_message(MessageType::INFO, message).await;
        }

        let doc = ParsedDocument {
            analyzed: result.analyzed,
            text: text.clone(),
            version: params.text_document.version,
            semantic_index,
        };

        self.project_cache
            .write()
            .unwrap()
            .update_document(uri.clone(), doc);

        self.client
            .publish_diagnostics(uri, result.diagnostics, None)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.content_changes[0].text.clone();

        let result = crate::parser::parse::<T>(&text, &uri);
        let (semantic_index, log_messages) =
            crate::analyzer::build_semantic_index(&result.analyzed, &text);

        for message in log_messages {
            self.client.log_message(MessageType::INFO, message).await;
        }

        let doc = ParsedDocument {
            analyzed: result.analyzed,
            text: text.clone(),
            version: params.text_document.version,
            semantic_index,
        };

        self.project_cache
            .write()
            .unwrap()
            .update_document(uri.clone(), doc);

        self.client
            .publish_diagnostics(uri, result.diagnostics, None)
            .await;
    }

    // async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    //     let position = params.text_document_position_params.position;
    //     let uri = params.text_document_position_params.text_document.uri;

    //     let doc = {
    //         let cache = self.project_cache.read().unwrap();
    //         match cache.documents.get(&uri) {
    //             Some(doc) => doc.clone(),
    //             None => return Ok(None),
    //         }
    //     };

    //     let hover_provider = HoverProvider::new(
    //         doc.text.clone(),
    //         doc.ast.clone(),
    //         doc.semantic_index.clone(),
    //     );

    //     let hover_result = hover_provider.get_hover(position);
    //     Ok(hover_result)
    // }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        // First log message
        self.client
            .log_message(
                MessageType::INFO,
                format!("Hover requested at position {:?} in {}", position, uri),
            )
            .await;

        let doc = {
            let cache = self.project_cache.read().unwrap();
            match cache.documents.get(&uri) {
                Some(doc) => doc.clone(),
                None => return Ok(None),
            }
        };

        self.client
            .log_message(MessageType::INFO, "Document found, creating hover provider")
            .await;

        let hover_provider = HoverProvider::new(
            doc.text.clone(),
            doc.analyzed.clone(), // TODO: this is ugly
            doc.semantic_index.clone(),
        );

        let (hover_result, log_messages) = hover_provider.get_hover(position);

        for message in log_messages {
            self.client.log_message(MessageType::INFO, message).await;
        }

        match &hover_result {
            Some(hover) => {
                if let HoverContents::Markup(content) = &hover.contents {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("Hover content generated: {}", content.value),
                        )
                        .await;
                }
            }
            None => {
                self.client
                    .log_message(MessageType::INFO, "No hover information found")
                    .await;
            }
        }

        Ok(hover_result)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend::<GoldilocksField> {
        client,
        project_cache: RwLock::new(ProjectCache::new()),
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
