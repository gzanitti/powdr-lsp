mod hover;
mod parser;

use std::sync::RwLock;

use hover::HoverProvider;
use parser::AnalyzedDoc;
use powdr::FieldElement;
use std::collections::HashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend<T: FieldElement> {
    client: Client,
    documents: RwLock<HashMap<Url, (AnalyzedDoc<T>, String)>>,
}

impl<T: FieldElement> Backend<T> {
    async fn analyze_document(&self, uri: Url, text: String) {
        let result = crate::parser::parse::<T>(&text, &uri);

        self.documents
            .write()
            .unwrap()
            .insert(uri.clone(), (result.analyzed, text.clone()));

        self.client
            .publish_diagnostics(uri, result.diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl<T: FieldElement> LanguageServer for Backend<T> {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
        self.analyze_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.analyze_document(
            params.text_document.uri,
            params.content_changes[0].text.clone(),
        )
        .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let (analyzed, text) = {
            let documents = self.documents.read().unwrap();
            match documents.get(&uri) {
                Some((analyzed, text)) => (analyzed.clone(), text.clone()),
                None => return Ok(None),
            }
        };

        let hover_provider = HoverProvider::new(text, analyzed);
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

    let (service, socket) = LspService::build(|client| Backend::<powdr::GoldilocksField> {
        client,
        documents: std::sync::RwLock::new(std::collections::HashMap::new()),
    })
    .finish();

    Server::new(stdin, stdout, socket).serve(service).await;
}
