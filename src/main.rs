mod hover;
mod parser;

use std::fs;
use std::path::Path;
use std::sync::RwLock;

use hover::HoverProvider;
use parser::AnalyzedDoc;
use powdr_number::{FieldElement, GoldilocksField};
use std::collections::HashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend<T: FieldElement> {
    client: Client,
    project_cache: RwLock<ProjectCache<T>>,
}

#[derive(Debug)]
struct ProjectCache<T> {
    // Store parsed files
    documents: HashMap<Url, ParsedDocument<T>>,
    // Global symbol table for quick lookups across files
    symbol_locations: HashMap<String, Vec<(Url, SymbolKind)>>,
}

#[derive(Debug, Clone)]
struct ParsedDocument<T> {
    ast: AnalyzedDoc<T>,
    text: String,
    version: i32,
}

#[derive(Debug, Clone, PartialEq)]
enum SymbolKind {
    Machine,
    Function,
    Register,
    Definition,
    Public,
    Intermediate,
    TraitImpl,
}

impl<T: FieldElement> ProjectCache<T> {
    fn new() -> Self {
        Self {
            documents: HashMap::new(),
            symbol_locations: HashMap::new(),
        }
    }

    fn update_document(&mut self, uri: Url, doc: ParsedDocument<T>) {
        // Remove old symbol entries for this document
        self.remove_document_symbols(&uri);

        // Index all symbols from the new document
        match &doc.ast {
            AnalyzedDoc::ASM(asm) => {
                for (name, _) in asm.machines() {
                    self.add_symbol(name.to_string(), uri.clone(), SymbolKind::Machine);
                }
                // Add other ASM symbols...
            }
            AnalyzedDoc::PIL(pil) => {
                for name in pil.definitions.keys() {
                    self.add_symbol(name.clone(), uri.clone(), SymbolKind::Definition);
                }
                for name in pil.public_declarations.keys() {
                    self.add_symbol(name.clone(), uri.clone(), SymbolKind::Public);
                }
                // Add other PIL symbols...
            }
        }

        self.documents.insert(uri, doc);
    }

    fn add_symbol(&mut self, name: String, uri: Url, kind: SymbolKind) {
        self.symbol_locations
            .entry(name)
            .or_default()
            .push((uri, kind));
    }

    fn remove_document_symbols(&mut self, uri: &Url) {
        for locations in self.symbol_locations.values_mut() {
            locations.retain(|(doc_uri, _)| doc_uri != uri);
        }
        // Clean up empty entries
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

                    let doc = ParsedDocument {
                        ast: result.analyzed,
                        text: content,
                        version: 0,
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
        self.client
            .log_message(
                MessageType::INFO,
                format!("Opening document: {}", params.text_document.uri),
            )
            .await;
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        let result = crate::parser::parse::<T>(&text, &uri);

        let doc = ParsedDocument {
            ast: result.analyzed,
            text: text.clone(),
            version: 0,
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
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Document changed: {} (version {})",
                    params.text_document.uri, params.text_document.version
                ),
            )
            .await;
        let uri = params.text_document.uri;
        let text = params.content_changes[0].text.clone();

        let result = crate::parser::parse::<T>(&text, &uri);

        let doc = ParsedDocument {
            ast: result.analyzed,
            text: text.clone(),
            version: params.text_document.version,
        };

        self.project_cache
            .write()
            .unwrap()
            .update_document(uri.clone(), doc);

        self.client
            .publish_diagnostics(uri, result.diagnostics, None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "Hover request at position {:?} in {}",
                    params.text_document_position_params.position,
                    params.text_document_position_params.text_document.uri
                ),
            )
            .await;
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let cache = self.project_cache.read().unwrap();
        let doc = match cache.documents.get(&uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let hover_provider = HoverProvider::new(doc.text.clone(), doc.ast.clone());
        let hover_result = hover_provider.get_hover(position);

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
