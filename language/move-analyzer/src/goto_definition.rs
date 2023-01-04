#![allow(dead_code)]
use super::context::*;
use super::item::*;
use super::modules::*;
use super::scopes::*;
use super::utils::in_range;
use crate::utils::discover_manifest_and_kind;
use crate::utils::path_concat;
use crate::utils::FileRange;
use crate::utils::GetPosition;
use lsp_server::*;
use lsp_types::*;
use move_compiler::shared::Identifier;
use move_ir_types::location::Loc;
use std::path::PathBuf;

/// Handles go-to-def request of the language server
pub fn on_go_to_def_request(context: &Context, request: &Request) {
    let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-def request");
    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;
    let fpath = path_concat(
        PathBuf::from(std::env::current_dir().unwrap()).as_path(),
        fpath.as_path(),
    );
    log::info!(
        "request is goto definition,fpath:{:?}  line:{} col:{}",
        fpath.as_path(),
        line,
        col,
    );

    let (manifest_dir, layout) = match discover_manifest_and_kind(fpath.as_path()) {
        Some(x) => x,
        None => {
            log::error!(
                "fpath:{:?} can't find manifest_dir or kind",
                fpath.as_path()
            );
            return;
        }
    };
    let mut visitor = Visitor::new(fpath.clone(), line, col);
    context
        .modules
        .run_visitor_for_file(&mut visitor, &manifest_dir, &fpath, layout);

    match &visitor.result {
        Some(x) => {
            let range = Range {
                start: Position {
                    line: x.line,
                    character: x.col_start,
                },
                end: Position {
                    line: x.line,
                    character: x.col_end,
                },
            };
            let uri = Url::from_file_path(x.path.as_path()).unwrap();
            let loc = GotoDefinitionResponse::Scalar(Location::new(uri, range));
            log::info!("found location is {:?}", loc);
            let r = Response::new_ok(request.id.clone(), serde_json::to_value(loc).unwrap());
            context
                .connection
                .sender
                .send(Message::Response(r))
                .unwrap();
        }
        None => {
            log::error!(
                "{:?}:{}:{} not found definition.",
                visitor.filepath,
                line,
                col
            );
        }
    }
}

pub(crate) struct Visitor {
    /// The file we are looking for.
    pub(crate) filepath: PathBuf,
    pub(crate) line: u32,
    pub(crate) col: u32,
    pub(crate) result: Option<FileRange>,
    /// result_loc not convert to a FileRange
    /// Current references find depend on this field.
    pub(crate) result_loc: Option<Loc>,
    ///
    pub(crate) result_item_or_access: Option<ItemOrAccess>,
}

impl Visitor {
    pub(crate) fn new(filepath: impl Into<PathBuf>, line: u32, col: u32) -> Self {
        Self {
            filepath: filepath.into(),
            line,
            col,
            result: None,
            result_loc: None,
            result_item_or_access: None,
        }
    }

    ///  match loc   
    fn match_loc(&self, loc: &Loc, services: &dyn HandleItemService) -> bool {
        let r = services.convert_loc_range(loc);
        match &r {
            Some(r) => r.in_range(self.filepath.clone(), self.line, self.col),
            None => false,
        }
    }
}

impl ScopeVisitor for Visitor {
    fn visit_fun_or_spec_body(&self) -> bool {
        true
    }
    fn handle_item_or_access(
        &mut self,
        services: &dyn HandleItemService,
        _scopes: &Scopes,
        item_or_access: &ItemOrAccess,
    ) {
        match item_or_access {
            ItemOrAccess::Item(item) => match item {
                Item::UseModule(name, alias, _, s) => {
                    if self.match_loc(&name.value.module.loc(), services)
                        || match alias {
                            Some(alias) => self.match_loc(&alias.0.loc, services),
                            None => false,
                        }
                        || match s {
                            Some(s) => self.match_loc(&s.loc, services),
                            _ => false,
                        }
                    {
                        if let Some(t) = services.convert_loc_range(&item.def_loc()) {
                            self.result = Some(t);
                            self.result_loc = Some(item.def_loc());
                            self.result_item_or_access = Some(item_or_access.clone());
                        }
                    }
                }
                Item::UseMember(module_name, name, alias, x) => {
                    if self.match_loc(&module_name.value.module.loc(), services) {
                        let module_loc = x
                            .as_ref()
                            .borrow()
                            .module
                            .module_name_and_addr
                            .as_ref()
                            .unwrap()
                            .name
                            .loc();
                        if let Some(t) = services.convert_loc_range(&module_loc) {
                            self.result = Some(t);
                            self.result_loc = Some(module_loc);
                            self.result_item_or_access = Some(item_or_access.clone());
                            return;
                        }
                    }
                    if self.match_loc(&name.loc, services)
                        || match alias {
                            Some(alias) => self.match_loc(&alias.loc, services),
                            None => false,
                        }
                    {
                        if let Some(t) = services.convert_loc_range(&item.def_loc()) {
                            self.result = Some(t);
                            self.result_loc = Some(item.def_loc());
                            self.result_item_or_access = Some(item_or_access.clone());
                        }
                    }
                }
                // If Some special add here.
                // Right now default is enough.
                _ => {
                    let loc = item.def_loc();
                    if self.match_loc(&loc, services) {
                        if let Some(t) = services.convert_loc_range(&loc) {
                            self.result = Some(t);
                            self.result_loc = Some(loc);
                            self.result_item_or_access = Some(item_or_access.clone());
                        }
                    }
                }
            },
            ItemOrAccess::Access(access) => match item_or_access {
                _ => {
                    log::trace!("access:{}", access);
                    if let Some((access, def)) = access.access_module() {
                        if self.match_loc(&access, services) {
                            if let Some(t) = services.convert_loc_range(&def) {
                                self.result = Some(t);
                                self.result_loc = Some(def);
                                self.result_item_or_access = Some(item_or_access.clone());
                                return;
                            }
                        }
                    }
                    let locs = access.access_def_loc();
                    if self.match_loc(&locs.0, services) {
                        if let Some(t) = services.convert_loc_range(&locs.1) {
                            self.result = Some(t);
                            self.result_loc = Some(locs.1);
                            self.result_item_or_access = Some(item_or_access.clone());
                        }
                    }
                }
            },
        }
    }

    fn function_or_spec_body_should_visit(&self, start: &FileRange, end: &FileRange) -> bool {
        in_range(self, start, end)
    }

    fn finished(&self) -> bool {
        self.result.is_some()
    }
}

impl std::fmt::Display for Visitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "goto_definition,file:{:?} line:{} col:{}",
            self.filepath, self.line, self.col
        )
    }
}
impl GetPosition for Visitor {
    fn get_position(&self) -> (PathBuf, u32, u32) {
        (self.filepath.clone(), self.line, self.col)
    }
}
