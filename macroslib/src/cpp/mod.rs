macro_rules! file_for_module {
    ($ctx:ident, $common_files:ident, $module_name:ident) => {{
        let output_dir = &$ctx.cfg.output_dir;
        let target_pointer_width = $ctx.target_pointer_width;
        let generated_foreign_files = &mut $ctx.generated_foreign_files;
        $common_files
            .entry($module_name.clone())
            .or_insert_with(|| {
                let c_header_path = output_dir.join($module_name.as_str());
                let mut c_header_f = FileWriteCache::new(&c_header_path, *generated_foreign_files);
                write!(
                    &mut c_header_f,
                    r##"// Automatically generated by flapigen
#pragma once

//for (u)intX_t types
#include <stdint.h>

#ifdef __cplusplus
static_assert(sizeof(uintptr_t) == sizeof(uint8_t) * {sizeof_usize},
   "our conversation usize <-> uintptr_t is wrong");
#endif
            "##,
                    sizeof_usize = target_pointer_width / 8,
                )
                .expect("write to memory failed, no free mem?");
                c_header_f
            })
    }};
}

mod cpp_code;
mod fclass;
mod fenum;
mod finterface;
mod map_class_self_type;
mod map_type;

use std::{io::Write, mem, path::PathBuf, rc::Rc};

use log::{debug, trace};
use proc_macro2::TokenStream;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;
use strum::IntoEnumIterator;
use syn::spanned::Spanned;

use crate::{
    cpp::{map_class_self_type::register_typemap_for_self_type, map_type::map_type},
    error::{invalid_src_id_span, DiagnosticError, Result},
    file_cache::FileWriteCache,
    source_registry::SourceId,
    typemap::{
        ast::{check_if_smart_pointer_return_inner_type, parse_ty_with_given_span, TypeName},
        ty::{ForeignConversationRule, ForeignType, ForeignTypeS, RustType},
        utils::{
            configure_ftype_rule, remove_files_if, validate_cfg_options, ForeignMethodSignature,
            ForeignTypeInfoT,
        },
        CItem, CItems, ForeignTypeInfo, TypeConvCode, TypeMapConvRuleInfo,
    },
    types::{ForeignClassInfo, ForeignMethod, ItemToExpand, MethodAccess, MethodVariant},
    CppConfig, CppOptional, CppStrView, CppVariant, LanguageGenerator, SourceCode, TypeMap,
    SMART_PTR_COPY_TRAIT, WRITE_TO_MEM_FAILED_MSG,
};

#[derive(Debug)]
struct CppConverter {
    typename: SmolStr,
    converter: Rc<TypeConvCode>,
}

#[derive(Debug)]
struct CppForeignTypeInfo {
    base: ForeignTypeInfo,
    provides_by_module: Vec<SmolStr>,
    input_to_output: bool,
    pub(in crate::cpp) cpp_converter: Option<CppConverter>,
}

impl ForeignTypeInfoT for CppForeignTypeInfo {
    fn name(&self) -> &str {
        self.base.name.as_str()
    }
    fn correspoding_rust_type(&self) -> &RustType {
        &self.base.correspoding_rust_type
    }
}

impl CppForeignTypeInfo {
    pub(in crate::cpp) fn try_new(
        ctx: &mut CppContext,
        direction: petgraph::Direction,
        ftype_idx: ForeignType,
    ) -> Result<Self> {
        let ftype = &ctx.conv_map[ftype_idx];
        let mut cpp_converter = None;

        let origin_ftype_span = ftype.src_id_span();

        let rule = match direction {
            petgraph::Direction::Outgoing => ftype.into_from_rust.as_ref(),
            petgraph::Direction::Incoming => ftype.from_into_rust.as_ref(),
        }
        .ok_or_else(|| {
            DiagnosticError::new2(
                origin_ftype_span,
                format!(
                    "No rule to convert foreign type {} as input/output type",
                    ftype.name
                ),
            )
        })?;
        let mut provides_by_module = ftype.provides_by_module.clone();
        let base_rt;
        let base_ft_name;
        let mut input_to_output = false;
        if let Some(intermediate) = rule.intermediate.as_ref() {
            input_to_output = intermediate.input_to_output;
            base_rt = intermediate.intermediate_ty;
            let typename = ftype.typename();
            let converter = intermediate.conv_code.clone();
            let intermediate_ty = intermediate.intermediate_ty;

            let rty = ctx.conv_map[intermediate_ty].clone();
            let arg_span = intermediate.conv_code.full_span();
            let inter_ft = map_type(ctx, &rty, direction, arg_span)?;
            if inter_ft.cpp_converter.is_some()
                || base_rt != inter_ft.base.correspoding_rust_type.to_idx()
            {
                return Err(DiagnosticError::new2(
                    origin_ftype_span,
                    format!(
                        "Error during conversation {} for {},\n
                    intermidiate type '{}' can not directrly converted to C",
                        typename,
                        match direction {
                            petgraph::Direction::Outgoing => "output",
                            petgraph::Direction::Incoming => "input",
                        },
                        rty
                    ),
                )
                .add_span_note(
                    invalid_src_id_span(),
                    if let Some(cpp_conv) = inter_ft.cpp_converter {
                        format!(
                            "it requires C++ code to convert from '{}' to '{}'",
                            inter_ft.base.name, cpp_conv.typename
                        )
                    } else {
                        format!(
                            "Type '{}' require conversation to type '{}' before usage as C type",
                            ctx.conv_map[base_rt], inter_ft.base.correspoding_rust_type
                        )
                    },
                ));
            }
            provides_by_module.extend_from_slice(&inter_ft.provides_by_module);
            base_ft_name = inter_ft.base.name;
            cpp_converter = Some(CppConverter {
                typename,
                converter,
            });
        } else {
            base_rt = rule.rust_ty;
            base_ft_name = ftype.typename();
        }
        trace!(
            "CppForeignTypeInfo::try_new base_ft_name {}, cpp_converter {:?}",
            base_ft_name,
            cpp_converter
        );
        Ok(CppForeignTypeInfo {
            input_to_output,
            base: ForeignTypeInfo {
                name: base_ft_name,
                correspoding_rust_type: ctx.conv_map[base_rt].clone(),
            },
            provides_by_module,
            cpp_converter,
        })
    }
}

impl AsRef<ForeignTypeInfo> for CppForeignTypeInfo {
    fn as_ref(&self) -> &ForeignTypeInfo {
        &self.base
    }
}

struct CppForeignMethodSignature {
    output: CppForeignTypeInfo,
    input: Vec<CppForeignTypeInfo>,
}

impl From<ForeignTypeInfo> for CppForeignTypeInfo {
    fn from(x: ForeignTypeInfo) -> Self {
        CppForeignTypeInfo {
            input_to_output: false,
            base: ForeignTypeInfo {
                name: x.name,
                correspoding_rust_type: x.correspoding_rust_type,
            },
            provides_by_module: Vec::new(),
            cpp_converter: None,
        }
    }
}

impl ForeignMethodSignature for CppForeignMethodSignature {
    type FI = CppForeignTypeInfo;
    fn output(&self) -> &dyn ForeignTypeInfoT {
        &self.output.base
    }
    fn input(&self) -> &[CppForeignTypeInfo] {
        &self.input[..]
    }
}

struct MethodContext<'a> {
    class: &'a ForeignClassInfo,
    method: &'a ForeignMethod,
    f_method: &'a CppForeignMethodSignature,
    c_func_name: &'a str,
    decl_func_args: &'a str,
    real_output_typename: &'a str,
    ret_name: &'a str,
}

impl CppConfig {
    fn register_class(&self, conv_map: &mut TypeMap, class: &ForeignClassInfo) -> Result<()> {
        class
            .validate_class()
            .map_err(|err| DiagnosticError::new(class.src_id, class.span(), err))?;
        if let Some(self_desc) = class.self_desc.as_ref() {
            let constructor_ret_type = &self_desc.constructor_ret_type;
            let this_type_for_method = constructor_ret_type;
            let mut traits = vec!["SwigForeignClass"];
            if class.clone_derived() {
                traits.push("Clone");
            }
            if class.copy_derived() {
                if !class.clone_derived() {
                    traits.push("Clone");
                }
                traits.push("Copy");
            }

            if class.smart_ptr_copy_derived() {
                traits.push(SMART_PTR_COPY_TRAIT);
            }

            let this_type = conv_map.find_or_alloc_rust_type_that_implements(
                this_type_for_method,
                &traits,
                class.src_id,
            );

            if class.smart_ptr_copy_derived() {
                if class.copy_derived() {
                    println!(
                        "cargo:warning=class {} marked as Copy and {}, ignore Copy",
                        class.name, SMART_PTR_COPY_TRAIT
                    );
                }
                if check_if_smart_pointer_return_inner_type(&this_type, "Rc").is_none()
                    && check_if_smart_pointer_return_inner_type(&this_type, "Arc").is_none()
                {
                    return Err(DiagnosticError::new(
                        class.src_id,
                        this_type.ty.span(),
                        format!(
                            "class {} marked as {}, but type '{}' is not Arc<> or Rc<>",
                            class.name, SMART_PTR_COPY_TRAIT, this_type
                        ),
                    ));
                }

                let has_clone = class.methods.iter().any(|x| match x.variant {
                    MethodVariant::Method(_) | MethodVariant::StaticMethod => {
                        x.rust_id.is_ident("clone")
                    }
                    MethodVariant::Constructor => false,
                });
                if has_clone {
                    return Err(DiagnosticError::new(
                        class.src_id,
                        this_type.ty.span(),
                        format!(
                            "class {} marked as {}, but has clone method. Error: can not generate clone method.",
                            class.name, SMART_PTR_COPY_TRAIT,
                        ),
                    ));
                }
            }

            register_typemap_for_self_type(conv_map, class, this_type, self_desc)?;
        }
        conv_map.find_or_alloc_rust_type(&class.self_type_as_ty(), class.src_id);
        Ok(())
    }
}

struct CppContext<'a> {
    cfg: &'a CppConfig,
    conv_map: &'a mut TypeMap,
    target_pointer_width: usize,
    rust_code: &'a mut Vec<TokenStream>,
    common_files: &'a mut FxHashMap<SmolStr, FileWriteCache>,
    generated_foreign_files: &'a mut FxHashSet<PathBuf>,
}

impl LanguageGenerator for CppConfig {
    fn expand_items(
        &self,
        conv_map: &mut TypeMap,
        target_pointer_width: usize,
        code: &[SourceCode],
        items: Vec<ItemToExpand>,
        remove_not_generated_files: bool,
    ) -> Result<Vec<TokenStream>> {
        let mut ret = Vec::with_capacity(items.len());
        let mut files = FxHashMap::<SmolStr, FileWriteCache>::default();
        let mut generated_foreign_files = FxHashSet::default();
        {
            let mut ctx = CppContext {
                cfg: self,
                conv_map,
                target_pointer_width,
                rust_code: &mut ret,
                common_files: &mut files,
                generated_foreign_files: &mut generated_foreign_files,
            };
            init(&mut ctx, code)?;
            for item in &items {
                if let ItemToExpand::Class(ref fclass) = item {
                    self.register_class(ctx.conv_map, fclass)?;
                }
            }
            for item in items {
                match item {
                    ItemToExpand::Class(fclass) => fclass::generate(&mut ctx, &fclass)?,
                    ItemToExpand::Enum(fenum) => fenum::generate_enum(&mut ctx, &fenum)?,
                    ItemToExpand::Interface(finterface) => {
                        finterface::generate_interface(&mut ctx, &finterface)?
                    }
                }
            }
        }

        for (module_name, c_header_f) in files {
            let c_header_path = self.output_dir.join(module_name.as_str());
            c_header_f.update_file_if_necessary().map_err(|err| {
                DiagnosticError::map_any_err_to_our_err(format!(
                    "write to {} failed: {}",
                    c_header_path.display(),
                    err
                ))
            })?;
        }

        if remove_not_generated_files {
            remove_files_if(&self.output_dir, |path| {
                if let Some(ext) = path.extension() {
                    if (ext == "h" || ext == "hpp") && !generated_foreign_files.contains(path) {
                        return true;
                    }
                }
                false
            })
            .map_err(DiagnosticError::map_any_err_to_our_err)?;
        }

        Ok(ret)
    }
}

fn c_func_name(class: &ForeignClassInfo, method: &ForeignMethod) -> String {
    do_c_func_name(class, method.access, &method.short_name())
}

fn do_c_func_name(
    class: &ForeignClassInfo,
    method_access: MethodAccess,
    method_short_name: &str,
) -> String {
    format!(
        "{access}{class_name}_{func}",
        access = match method_access {
            MethodAccess::Private => "private_",
            MethodAccess::Protected => "protected_",
            MethodAccess::Public => "",
        },
        class_name = class.name,
        func = method_short_name,
    )
}

fn rust_generate_args_with_types(f_method: &CppForeignMethodSignature) -> String {
    use std::fmt::Write;

    let mut buf = String::new();
    for (i, f_type_info) in f_method.input.iter().enumerate() {
        write!(
            &mut buf,
            "a{}: {}, ",
            i,
            f_type_info.as_ref().correspoding_rust_type.typename(),
        )
        .expect(WRITE_TO_MEM_FAILED_MSG);
    }
    buf
}

fn register_c_type(
    tmap: &mut TypeMap,
    c_types: &CItems,
    fcode: &FileWriteCache,
    src_id: SourceId,
) -> Result<bool> {
    let mut something_defined = false;
    for c_type in &c_types.items {
        let (f_ident, c_name) = match c_type {
            CItem::Struct(ref s) => (&s.ident, format!("struct {}", s.ident)),
            CItem::Union(ref u) => (&u.ident, format!("union {}", u.ident)),
            CItem::Fn(_) => continue,
        };
        if fcode.is_item_defined(&c_name) {
            continue;
        }
        something_defined = true;
        let rust_ty = parse_ty_with_given_span(&f_ident.to_string(), f_ident.span())
            .map_err(|err| DiagnosticError::from_syn_err(src_id, err))?;
        let rust_ty = tmap.find_or_alloc_rust_type(&rust_ty, src_id);
        debug!("init::c_types add {} / {}", rust_ty, c_name);
        if let Some(ftype_idx) = tmap.find_foreign_type_related_to_rust_ty(rust_ty.to_idx()) {
            if tmap[ftype_idx].name.as_str() != c_name {
                return Err(DiagnosticError::new(
                    src_id,
                    f_ident.span(),
                    format!(
                        "There is already exists foreign type related to rust type '{}', \
                         but name is different: should be {}, have {}",
                        rust_ty,
                        c_name,
                        tmap[ftype_idx].name.as_str()
                    ),
                ));
            }
        } else {
            let rule = ForeignConversationRule {
                rust_ty: rust_ty.to_idx(),
                intermediate: None,
            };
            tmap.alloc_foreign_type(ForeignTypeS {
                name: TypeName::new(c_name, (src_id, f_ident.span())),
                provides_by_module: vec![format!("\"{}\"", c_types.header_name).into()],
                into_from_rust: Some(rule.clone()),
                from_into_rust: Some(rule),
                name_prefix: None,
            })?;
        }
    }
    Ok(something_defined)
}

fn merge_rule(ctx: &mut CppContext, mut rule: TypeMapConvRuleInfo) -> Result<()> {
    debug!("merge_rule begin {:?}", rule);
    if rule.is_empty() {
        return Err(DiagnosticError::new(
            rule.src_id,
            rule.span,
            format!("rule {:?} is empty", rule),
        ));
    }
    let all_options = {
        let mut opts = FxHashSet::<&'static str>::default();
        opts.extend(CppOptional::iter().map(|x| -> &'static str { x.into() }));
        opts.extend(CppVariant::iter().map(|x| -> &'static str { x.into() }));
        opts.extend(CppStrView::iter().map(|x| -> &'static str { x.into() }));
        opts
    };

    validate_cfg_options(&rule, &all_options)?;
    let options = {
        let mut opts = FxHashSet::<&'static str>::default();
        opts.insert(ctx.cfg.cpp_variant.into());
        opts.insert(ctx.cfg.cpp_optional.into());
        opts.insert(ctx.cfg.cpp_str_view.into());
        opts
    };

    if let Some(c_types) = rule.c_types.take() {
        merge_c_types(ctx, c_types, MergeCItemsFlags::DefineOnlyCItem, rule.src_id)?;
    }

    let f_codes = mem::replace(&mut rule.f_code, vec![]);
    for fcode in f_codes {
        let module_name = &fcode.module_name;
        let common_files = &mut ctx.common_files;
        let c_header_f = file_for_module!(ctx, common_files, module_name);
        let use_fcode = fcode
            .cfg_option
            .as_ref()
            .map(|opt| options.contains(opt.as_str()))
            .unwrap_or(true);

        if use_fcode {
            c_header_f
                .write_all(
                    fcode
                        .code
                        .replace("$RUST_SWIG_USER_NAMESPACE", &ctx.cfg.namespace_name)
                        .as_bytes(),
                )
                .map_err(DiagnosticError::map_any_err_to_our_err)?;
        }
    }

    configure_ftype_rule(&mut rule.ftype_left_to_right, "=>", rule.src_id, &options)?;
    configure_ftype_rule(&mut rule.ftype_right_to_left, "<=", rule.src_id, &options)?;

    ctx.conv_map.merge_conv_rule(rule.src_id, rule)?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum MergeCItemsFlags {
    DefineAlsoRustType,
    DefineOnlyCItem,
}

fn merge_c_types(
    ctx: &mut CppContext,
    c_types: CItems,
    flags: MergeCItemsFlags,
    rule_src_id: SourceId,
) -> Result<()> {
    {
        let module_name = &c_types.header_name;
        let common_files = &mut ctx.common_files;
        let c_header_f = file_for_module!(ctx, common_files, module_name);
        register_c_type(ctx.conv_map, &c_types, c_header_f, rule_src_id)?;
    }
    cpp_code::generate_c_type(ctx, &c_types, flags, rule_src_id)?;

    Ok(())
}

fn init(ctx: &mut CppContext, code: &[SourceCode]) -> Result<()> {
    if !(ctx.cfg.output_dir.exists() && ctx.cfg.output_dir.is_dir()) {
        return Err(DiagnosticError::map_any_err_to_our_err(format!(
            "Path {} not exists or not directory",
            ctx.cfg.output_dir.display()
        )));
    }
    //for enum
    ctx.conv_map
        .find_or_alloc_rust_type_no_src_id(&parse_type! { u32 });

    for cu in code {
        let src_path = ctx.cfg.output_dir.join(&cu.id_of_code);
        let mut src_file = FileWriteCache::new(&src_path, ctx.generated_foreign_files);
        src_file
            .write_all(
                cu.code
                    .replace("RUST_SWIG_USER_NAMESPACE", &ctx.cfg.namespace_name)
                    .as_bytes(),
            )
            .map_err(|err| {
                DiagnosticError::map_any_err_to_our_err(format!(
                    "write to {} failed: {}",
                    src_path.display(),
                    err
                ))
            })?;
        src_file.update_file_if_necessary().map_err(|err| {
            DiagnosticError::map_any_err_to_our_err(format!(
                "update of {} failed: {}",
                src_path.display(),
                err
            ))
        })?;
    }

    let not_merged_data = ctx.conv_map.take_not_merged_not_generic_rules();
    for rule in not_merged_data {
        merge_rule(ctx, rule)?;
    }

    Ok(())
}
