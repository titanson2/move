// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, ops::Deref};

use crate::{
    ast::{Operation, PropertyBag, PropertyValue, QualifiedSymbol},
    builder::module_builder::SpecBlockContext,
    model::{IntrinsicId, QualifiedId, SpecFunId},
    pragmas::{INTRINSIC_PRAGMA, INTRINSIC_TYPE_MAP, INTRINSIC_TYPE_MAP_ASSOC_FUNCTIONS},
    symbol::Symbol,
    FunId, Loc, ModuleBuilder, StructId,
};

/// An information pack that holds the intrinsic declaration
#[derive(Clone, Debug)]
pub struct IntrinsicDecl {
    move_type: QualifiedId<StructId>,
    #[allow(dead_code)]
    intrinsic_type: Symbol,
    intrinsic_to_move_fun: BTreeMap<Symbol, QualifiedId<FunId>>,
    move_fun_to_intrinsic: BTreeMap<QualifiedId<FunId>, Symbol>,
    intrinsic_to_spec_fun: BTreeMap<Symbol, QualifiedId<SpecFunId>>,
    spec_fun_to_intrinsic: BTreeMap<QualifiedId<SpecFunId>, Symbol>,
}

pub(crate) fn process_intrinsic_declaration(
    builder: &mut ModuleBuilder,
    loc: &Loc,
    context: &SpecBlockContext,
    props: &mut PropertyBag,
) {
    // intrinsic declarations only appears in struct spec block
    let type_qsym = match context {
        SpecBlockContext::Struct(qsym) => qsym.clone(),
        _ => {
            return;
        }
    };

    // search for intrinsic declarations
    let symbol_pool = builder.symbol_pool();
    let pragma_symbol = symbol_pool.make(INTRINSIC_PRAGMA);
    let target = match props.get_mut(&pragma_symbol) {
        None => {
            // this is not an intrinsic declaration
            return;
        }
        Some(val) => {
            match val {
                PropertyValue::Symbol(sym) => symbol_pool.string(*sym),
                PropertyValue::QualifiedSymbol(_) => {
                    builder
                        .parent
                        .error(loc, "expect a boolean value or a valid intrinsic type");
                    return;
                }
                _ => {
                    // this is the true/false pragma
                    return;
                }
            }
        }
    };

    // obtain the associated functions map
    let associated_funs = match target.as_str() {
        INTRINSIC_TYPE_MAP => INTRINSIC_TYPE_MAP_ASSOC_FUNCTIONS.deref(),
        _ => {
            builder
                .parent
                .error(loc, &format!("unknown intrinsic type: {}", target.as_str()));
            return;
        }
    };

    // prepare the decl
    let type_entry = builder.parent.struct_table.get(&type_qsym).expect("struct");
    let move_type = type_entry.module_id.qualified(type_entry.struct_id);

    let mut decl = IntrinsicDecl {
        move_type,
        intrinsic_type: symbol_pool.make(target.as_str()),
        intrinsic_to_move_fun: BTreeMap::new(),
        move_fun_to_intrinsic: BTreeMap::new(),
        intrinsic_to_spec_fun: BTreeMap::new(),
        spec_fun_to_intrinsic: BTreeMap::new(),
    };

    // construct the pack
    populate_intrinsic_decl(builder, loc, associated_funs, props, &mut decl);

    // add the decl back
    builder.parent.intrinsics.push(decl);
}

fn populate_intrinsic_decl(
    builder: &mut ModuleBuilder,
    loc: &Loc,
    associated_funs: &BTreeMap<&str, bool>,
    props: &mut PropertyBag,
    decl: &mut IntrinsicDecl,
) {
    let symbol_pool = builder.symbol_pool();
    for (&name, &is_move_fun) in associated_funs {
        let key_sym = symbol_pool.make(name);

        // look-up the target of the declaration, if present
        let target_sym = match props.remove(&key_sym) {
            None => {
                continue;
            }
            Some(PropertyValue::Value(_)) => {
                builder.parent.error(
                    loc,
                    &format!("invalid intrinsic function mapping: {}", name),
                );
                continue;
            }
            Some(PropertyValue::Symbol(val_sym)) => val_sym,
            Some(PropertyValue::QualifiedSymbol(qual_sym)) => {
                if qual_sym.module_name != builder.module_name {
                    builder.parent.error(
                        loc,
                        &format!(
                            "an intrinsic function mapping can only refer to functions \
                            declared in the same module while `{}` is not",
                            qual_sym.display(symbol_pool)
                        ),
                    );
                    continue;
                }
                qual_sym.symbol
            }
        };
        let qualified_sym = QualifiedSymbol {
            module_name: builder.module_name.clone(),
            symbol: target_sym,
        };

        // check presence
        if is_move_fun {
            match builder.parent.fun_table.get(&qualified_sym) {
                None => {
                    builder.parent.error(
                        loc,
                        &format!(
                            "unable to find move function for intrinsic mapping: {}",
                            qualified_sym.display(symbol_pool)
                        ),
                    );
                    continue;
                }
                Some(entry) => {
                    // TODO: in theory, we should also do some type checking on the function
                    // signature. This is implicitly done by Boogie right now, but we may want to
                    // make it more explicit and do the checking ourselves.
                    let qid = entry.module_id.qualified(entry.fun_id);
                    decl.intrinsic_to_move_fun.insert(key_sym, qid);
                    if decl.move_fun_to_intrinsic.insert(qid, key_sym).is_some() {
                        builder.parent.error(
                            loc,
                            &format!(
                                "duplicated intrinsic mapping for move function: {}",
                                qualified_sym.display(symbol_pool)
                            ),
                        );
                        continue;
                    }
                }
            }
        } else {
            match builder.parent.spec_fun_table.get(&qualified_sym) {
                None => {
                    builder.parent.error(
                        loc,
                        &format!(
                            "unable to find spec function for intrinsic mapping: {}",
                            qualified_sym.display(symbol_pool)
                        ),
                    );
                    continue;
                }
                Some(entries) => {
                    if entries.len() != 1 {
                        builder.parent.error(
                            loc,
                            &format!(
                                "unable to find a unique spec function for intrinsic mapping: {}",
                                qualified_sym.display(symbol_pool)
                            ),
                        );
                        continue;
                    }
                    let entry = &entries[0];

                    // TODO: in theory, we should also do some type checking on the function
                    // signature. This is implicitly done by Boogie right now, but we may want to
                    // make it more explicit and do the checking ourselves.
                    if let Operation::Function(mid, fid, ..) = &entry.oper {
                        let qid = mid.qualified(*fid);
                        decl.intrinsic_to_spec_fun.insert(key_sym, qid);
                        if decl.spec_fun_to_intrinsic.insert(qid, key_sym).is_some() {
                            builder.parent.error(
                                loc,
                                &format!(
                                    "duplicated intrinsic mapping for spec function: {}",
                                    qualified_sym.display(symbol_pool)
                                ),
                            );
                            continue;
                        }
                    }
                }
            }
        }
    }
}

/// Hosts all intrinsic declarations
#[derive(Clone, Debug, Default)]
pub struct IntrinsicsAnnotation {
    /// Intrinsic declarations
    decls: BTreeMap<IntrinsicId, IntrinsicDecl>,
    /// A map from intrinsic types to intrinsic decl
    intrinsic_structs: BTreeMap<QualifiedId<StructId>, IntrinsicId>,
    /// A map from intrinsic move functions to intrinsic decl
    intrinsic_move_funs: BTreeMap<QualifiedId<FunId>, IntrinsicId>,
    /// A map from intrinsic spec functions to intrinsic decl
    intrinsic_spec_funs: BTreeMap<QualifiedId<SpecFunId>, IntrinsicId>,
}

impl IntrinsicsAnnotation {
    /// Add a declaration pack into the annotation set
    pub fn add_decl(&mut self, decl: &IntrinsicDecl) {
        let id = IntrinsicId::new(self.decls.len());
        self.intrinsic_structs.insert(decl.move_type, id);
        for move_fid in decl.move_fun_to_intrinsic.keys() {
            self.intrinsic_move_funs.insert(*move_fid, id);
        }
        for spec_fid in decl.spec_fun_to_intrinsic.keys() {
            self.intrinsic_spec_funs.insert(*spec_fid, id);
        }
        self.decls.insert(id, decl.clone());
    }
}
