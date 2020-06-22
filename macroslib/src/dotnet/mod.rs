mod classes;
mod map_type;

use super::*;
use ast::{TypeName};
use error::{ResultDiagnostic, ResultSynDiagnostic, invalid_src_id_span};
use file_cache::FileWriteCache;
use heck::CamelCase;
use itertools::Itertools;
use map_type::{DotNetForeignMethodSignature, NameGenerator};
use quote::quote;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    rc::Rc,
};
use syn::{parse_str, Ident};
use typemap::{
    ast,
    ty::{ForeignConversationIntermediate, ForeignConversationRule, ForeignTypeS},
    TypeConvCode, FROM_VAR_TEMPLATE, TO_VAR_TEMPLATE,
};
use types::{
    ForeignEnumInfo, ForeignerClassInfo, ForeignerMethod, MethodVariant,
};

pub struct DotNetGenerator<'a> {
    config: &'a DotNetConfig,
    conv_map: &'a mut TypeMap,
    rust_code: Vec<TokenStream>,
    cs_file: FileWriteCache,
    additional_cs_code_for_types: HashMap<SmolStr, String>,
    known_c_items_modules: HashSet<SmolStr>,
}

impl<'a> DotNetGenerator<'a> {
    fn new(config: &'a DotNetConfig, conv_map: &'a mut TypeMap) -> Result<Self> {
        let mut generated_files_registry = FxHashSet::default();
        let cs_file = Self::create_cs_project(config, &mut generated_files_registry)?;

        Ok(Self {
            config,
            conv_map,
            rust_code: Vec::new(),
            cs_file,
            additional_cs_code_for_types: HashMap::new(),
            known_c_items_modules: HashSet::new(),
        })
    }

    fn generate(mut self, items: Vec<ItemToExpand>) -> Result<Vec<TokenStream>> {
        for item in &items {
            match item {
                ItemToExpand::Class(class) => {
                    classes::register_class(self.conv_map, class)?;
                }
                ItemToExpand::Enum(fenum) => self.generate_enum(fenum)?,
                _ => unimplemented!("Interfaces not supported yet for .NET"),
            }
        }
        for item in items {
            match item {
                ItemToExpand::Class(fclass) => {
                    self.generate_class_methods(&fclass)?;
                }
                ItemToExpand::Enum(_) => (),
                _ => unimplemented!("Interfaces not supported yet for .NET"),
            }
        }

        self.finish()?;
        self.cs_file.update_file_if_necessary()?;
        Ok(self.rust_code)
    }

    fn create_cs_project(
        config: &'a DotNetConfig,
        generated_files_registry: &mut FxHashSet<PathBuf>,
    ) -> Result<FileWriteCache> {
        fs::create_dir_all(&config.managed_lib_name).expect("Can't create managed lib directory");

        let mut csproj = File::create(format!("{0}/{0}.csproj", config.managed_lib_name))
            .with_note("Can't create csproj file")?;

        write!(
            csproj,
            r#"
<Project Sdk="Microsoft.NET.Sdk">

<PropertyGroup>
    <TargetFramework>netstandard2.0</TargetFramework>
</PropertyGroup>

</Project>
"#,
        )
        .with_note("Can't write to csproj file")?;

        let cs_file_name = config.managed_lib_name.clone() + ".cs";
        let mut cs_file = FileWriteCache::new(
            PathBuf::from(&config.managed_lib_name).join(cs_file_name),
            generated_files_registry,
        );

        write!(
            cs_file,
            r#"
// Generated by rust_swig. Do not edit.

using System;
using System.Runtime.InteropServices;

namespace {managed_lib_name}
{{
    internal static class RustString {{
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void c_string_delete(IntPtr c_char_ptr);

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern /* *mut RustString */ IntPtr c_str_u16_to_string(/* *const u16 */ IntPtr c_string_ptr);

        internal static string rust_to_dotnet(/* *const u16 */ IntPtr c_string_ptr)
        {{
            var dotnet_str = Marshal.PtrToStringUni(c_string_ptr);
            RustString.c_string_delete(c_string_ptr);
            return dotnet_str;
        }}

        internal static /* *mut RustString */ IntPtr dotnet_to_rust(string dotnet_str)
        {{
            var c_string_ptr = Marshal.StringToHGlobalUni(dotnet_str);
            var rust_string_ptr = c_str_u16_to_string(c_string_ptr);
            Marshal.FreeHGlobal(c_string_ptr);
            return rust_string_ptr;
        }}
    }}

    [System.Serializable]
    public class Error : System.Exception
    {{
        public Error(string message) : base(message) {{ }}
    }}
"#,
            managed_lib_name = config.managed_lib_name,
            native_lib_name = config.native_lib_name,
        )
        .with_note("Write to memory failed")?;

        Ok(cs_file)
    }

    fn generate_enum(&mut self, fenum: &ForeignEnumInfo) -> Result<()> {
        let enum_name = &fenum.name;
        let enum_variants = fenum
            .items
            .iter()
            .enumerate()
            .map(|(i, enum_item)| format!("{} = {}", enum_item.name.to_string().to_camel_case(), i))
            .join(",");
        let docstring = fenum.doc_comments.iter().map(|doc_line| {
            "/// ".to_owned() + doc_line
        }).join("\n");
        write!(
            self.cs_file,            
            r#"
    {docstring}
    public enum {enum_name} {{
        {enum_variants}
    }}"#,
            docstring = docstring,
            enum_name = enum_name,
            enum_variants = enum_variants,
        )
        .with_note("Write to memory failed")?;

        let span = fenum.span();

        let enum_type = self.conv_map.find_or_alloc_rust_type(
            &parse_type_spanned_checked!(span, #enum_name),
            fenum.src_id,
        );
        let intermediate_type = self.conv_map.find_or_alloc_rust_type(
            &parse_type_spanned_checked!(span, /* #enum_name */ u32),
            fenum.src_id,
        );

        self.conv_map.alloc_foreign_type(ForeignTypeS {
            name: TypeName::new(enum_name.to_string(), (fenum.src_id, fenum.name.span())),
            provides_by_module: vec![],
            into_from_rust: Some(ForeignConversationRule {
                rust_ty: enum_type.to_idx(),
                intermediate: Some(ForeignConversationIntermediate {
                    input_to_output: false,
                    intermediate_ty: intermediate_type.to_idx(),
                    conv_code: Rc::new(TypeConvCode::new(
                        format!(
                            "({enum_name}){from}",
                            enum_name = enum_name,
                            from = FROM_VAR_TEMPLATE,
                        ),
                        invalid_src_id_span(),
                    )),
                }),
            }),
            from_into_rust: Some(ForeignConversationRule {
                rust_ty: enum_type.to_idx(),
                intermediate: Some(ForeignConversationIntermediate {
                    input_to_output: false,
                    intermediate_ty: intermediate_type.to_idx(),
                    conv_code: Rc::new(TypeConvCode::new(
                        format!("(uint){from}", from = FROM_VAR_TEMPLATE),
                        invalid_src_id_span(),
                    )),
                }),
            }),
            name_prefix: None,
        })?;

        self.conv_map.alloc_foreign_type(ForeignTypeS {
            name: TypeName::new(
                format!("/* {} */ uint", enum_name),
                (fenum.src_id, fenum.name.span()),
            ),
            provides_by_module: vec![],
            into_from_rust: Some(ForeignConversationRule {
                rust_ty: intermediate_type.to_idx(),
                intermediate: None,
            }),
            from_into_rust: Some(ForeignConversationRule {
                rust_ty: intermediate_type.to_idx(),
                intermediate: None,
            }),
            name_prefix: None,
        })?;

        let (arms_to_u32, arms_from_u32): (Vec<_>, Vec<_>) = fenum.items.iter().enumerate().map(|(i, item)| {
            let item_name = &item.rust_name;
            let idx = i as u32;
            (quote! { #item_name => #idx }, quote! { #idx => #item_name })
        }).unzip();

        let rust_enum_name = &fenum.name;
        self.rust_code.push(quote! {
            impl SwigForeignEnum for #rust_enum_name {
                fn as_u32(&self) -> u32 {
                    match *self {
                        #(#arms_to_u32),*
                    }
                }
                fn from_u32(x: u32) -> Self {
                    match x {
                        #(#arms_from_u32),*
                        ,
                        _ => panic!(concat!("{} not expected for ", stringify!(#rust_enum_name)), x),
                    }
                }
            }
        });

        self.conv_map.add_conversation_rule(
            intermediate_type.to_idx(),
            enum_type.to_idx(),
            TypeConvCode::new2(
                format!("let {} = {}::from_u32({});", TO_VAR_TEMPLATE, enum_name, FROM_VAR_TEMPLATE),
                invalid_src_id_span(),
            )
            .into(),
        );

        self.conv_map.add_conversation_rule(
            enum_type.to_idx(),
            intermediate_type.to_idx(),
            TypeConvCode::new2(
                format!("let {} = {}.as_u32();", TO_VAR_TEMPLATE, FROM_VAR_TEMPLATE),
                invalid_src_id_span(),
            )
            .into(),
        );

        Ok(())
    }

    fn generate_class_methods(&mut self, class: &ForeignerClassInfo) -> Result<()> {
        self.generate_rust_destructor(class)?;
        self.generate_dotnet_class_code(class)?;

        for method in &class.methods {
            self.generate_method(&class, method)?;
        }

        writeln!(self.cs_file, "}} // class").with_note("Write to memory failed")?;

        Ok(())
    }

    fn generate_rust_destructor(&mut self, class: &ForeignerClassInfo) -> Result<()> {
        // Do not generate destructor for static classes.
        if let Some(self_desc) = class.self_desc.as_ref() {
            let class_name = &class.name;
            let storage_ty = &self_desc.constructor_ret_type;
            let storage_type = self
                .conv_map
                .find_or_alloc_rust_type(storage_ty, class.src_id);
            let smart_ptr_type =
                classes::SmartPointerType::new(&storage_type, self.conv_map, class.src_id);
            let intermediate_ptr_type = smart_ptr_type.intermediate_ptr_ty(storage_ty);
            let destructor_name = parse_str::<Ident>(&format!("{}_delete", class_name)).unwrap();

            let destructor_code = quote! {
                #[allow(non_snake_case, unused_variables, unused_mut, unused_unsafe)]
                #[no_mangle]
                unsafe extern "C" fn #destructor_name(this: #intermediate_ptr_type) {
                    ::std::mem::drop(Box::from_raw(this))
                }
            };
            self.rust_code.push(destructor_code);
        }
        Ok(())
    }

    fn generate_dotnet_class_code(&mut self, class: &ForeignerClassInfo) -> Result<()> {
        let class_name = class.name.to_string();
        let docstring = class.doc_comments.iter().map(|doc_line| {
            "/// ".to_owned() + doc_line
        }).join("\n");

        if let Some(_) = class.self_desc {
            let rust_destructor_name = class_name.clone() + "_delete";

            write!(
                self.cs_file,
                r#"
    {docstring}
    public class {class_name}: IDisposable {{
        internal IntPtr nativePtr;

        internal {class_name}(IntPtr nativePtr) {{
            this.nativePtr = nativePtr;
        }}

        public void Dispose() {{
            DoDispose();
            GC.SuppressFinalize(this);
        }}

        private void DoDispose() {{
            if (nativePtr != IntPtr.Zero) {{
                {rust_destructor_name}(nativePtr);
                nativePtr = IntPtr.Zero;
            }}
        }}

        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void {rust_destructor_name}(IntPtr __this);

        ~{class_name}() {{
            DoDispose();
        }}
"#,
                docstring = docstring,
                class_name = class_name,
                rust_destructor_name = rust_destructor_name,
                native_lib_name = self.config.native_lib_name,
            )
            .with_note("Write to memory failed")?;
        } else {
            writeln!(
                self.cs_file,
                "{docstring}\npublic static class {class_name} {{",
                docstring = docstring,
                class_name = class_name,
            )
            .with_note("Write to memory failed")?;
        }

        Ok(())
    }

    fn generate_method(
        &mut self,
        class: &ForeignerClassInfo,
        method: &ForeignerMethod,
    ) -> Result<()> {
        if method.is_dummy_constructor() {
            return Ok(());
        }
        let foreign_method_signature =
            map_type::make_foreign_method_signature(self, class, method)?;

        self.write_rust_glue_code(class, &foreign_method_signature)?;
        self.write_pinvoke_function_signature(class, &foreign_method_signature)?;
        self.write_dotnet_wrapper_function(class, &foreign_method_signature)?;

        Ok(())
    }

    fn write_rust_glue_code(
        &mut self,
        class: &ForeignerClassInfo,
        foreign_method_signature: &DotNetForeignMethodSignature,
    ) -> Result<()> {
        let method_name = &foreign_method_signature.name;
        let full_method_name = format!("{}_{}", class.name, method_name);

        let convert_input_code = itertools::process_results(
            foreign_method_signature
                .input
                .iter()
                .map(|arg| -> Result<String> {
                    let (mut deps, conversion) = arg.rust_conversion_code(self.conv_map)?;
                    self.rust_code.append(&mut deps);
                    Ok(conversion)
                }),
            |mut iter| iter.join(""),
        )?;

        let rust_func_args_str = foreign_method_signature
            .input
            .iter()
            .map(|arg_info| {
                format!(
                    "{}: {}",
                    arg_info.arg_name.rust_variable_name(),
                    arg_info.type_info.rust_intermediate_type.typename()
                )
            })
            .join(", ");

        let (mut deps, convert_output_code) = foreign_method_signature
            .output
            .rust_conversion_code(self.conv_map)?;
        self.rust_code.append(&mut deps);

        let rust_code_str = format!(
            r#"
    #[allow(non_snake_case, unused_variables, unused_mut, unused_unsafe)]
    #[no_mangle]
    pub extern "C" fn {func_name}({func_args}) -> {return_type} {{
        {convert_input_code}
        let mut {ret_name} = {call};
        {convert_output_code}
        {ret_name}
    }}
"#,
            func_name = full_method_name,
            func_args = rust_func_args_str,
            return_type = foreign_method_signature
                .output
                .type_info
                .rust_intermediate_type,
            convert_input_code = convert_input_code,
            ret_name = foreign_method_signature
                .output
                .arg_name
                .rust_variable_name(),
            convert_output_code = convert_output_code,
            call = foreign_method_signature.rust_function_call,
        );
        self.rust_code
            .push(syn::parse_str(&rust_code_str).with_syn_src_id(class.src_id)?);
        Ok(())
    }

    fn write_pinvoke_function_signature(
        &mut self,
        class: &ForeignerClassInfo,
        foreign_method_signature: &DotNetForeignMethodSignature,
    ) -> Result<()> {
        let method_name = &foreign_method_signature.name;
        let full_method_name = format!("{}_{}", class.name, method_name);
        let pinvoke_args_str = foreign_method_signature
            .input
            .iter()
            .map(|a| {
                format!(
                    "{} {}",
                    a.type_info.dotnet_intermediate_type,
                    a.arg_name.dotnet_variable_name()
                )
            })
            .join(", ");
        write!(
            self.cs_file,
            r#"
        [DllImport("{native_lib_name}", CallingConvention = CallingConvention.Cdecl)]
        internal static extern {return_type} {method_name}({args});
"#,
            native_lib_name = self.config.native_lib_name,
            return_type = foreign_method_signature
                .output
                .type_info
                .dotnet_intermediate_type,
            method_name = full_method_name,
            args = pinvoke_args_str,
        )
        .with_note("Write to memory failed")?;

        Ok(())
    }

    fn write_dotnet_wrapper_function(
        &mut self,
        class: &ForeignerClassInfo,
        foreign_method_signature: &DotNetForeignMethodSignature,
    ) -> Result<()> {
        let mut name_generator = NameGenerator::new();
        let maybe_static_str = if foreign_method_signature.variant == MethodVariant::StaticMethod {
            "static"
        } else {
            ""
        };
        let is_constructor = foreign_method_signature.variant == MethodVariant::Constructor;
        let full_method_name = format!("{}_{}", class.name, foreign_method_signature.name);
        let method_name = if is_constructor {
            "".to_owned()
        } else {
            foreign_method_signature.name.to_camel_case()
        };
        let args_to_skip = if let MethodVariant::Method(_) = foreign_method_signature.variant {
            1
        } else {
            0
        };
        let dotnet_args_str = foreign_method_signature
            .input
            .iter()
            .skip(args_to_skip)
            .map(|arg| {
                format!(
                    "{} {}",
                    arg.type_info.dotnet_type,
                    NameGenerator::first_variant(arg.arg_name.dotnet_variable_name())
                )
            })
            .join(", ");

        let this_input_conversion =
            if let MethodVariant::Method(_) = foreign_method_signature.variant {
                "var __this_0 = this.nativePtr;\n"
            } else {
                ""
            };

        let dotnet_input_conversion = this_input_conversion.to_owned()
            + &foreign_method_signature
                .input
                .iter()
                .skip(args_to_skip)
                .map(|arg| arg.dotnet_conversion_code(&mut name_generator))
                .join("\n            ");

        let returns_something =
            foreign_method_signature.output.type_info.dotnet_type != "void" && !is_constructor;
        let maybe_return_bind = if returns_something {
            "var __ret_0 = "
        } else if is_constructor {
            "this.nativePtr = "
        } else {
            ""
        };
        let maybe_dotnet_output_conversion = if returns_something {
            foreign_method_signature
                .output
                .dotnet_conversion_code(&mut name_generator)
        } else {
            String::new()
        };
        let maybe_return = if returns_something {
            format!("return {};", name_generator.last_variant("__ret"))
        } else {
            String::new()
        };

        let pinvoke_call_args = foreign_method_signature
            .input
            .iter()
            .map(|arg| name_generator.last_variant(arg.arg_name.dotnet_variable_name()))
            .join(", ");
        write!(
            self.cs_file,
            r#"
        {docstring}
        public {maybe_static} {dotnet_return_type} {method_name}({dotnet_args}) {{
            {dotnet_input_conversion}
            {maybe_return_bind}{full_method_name}({pinvoke_call_args});
            {maybe_dotnet_output_conversion}
            {maybe_return}
        }}
"#,
            docstring = foreign_method_signature.docstring,
            maybe_static = maybe_static_str,
            dotnet_return_type = foreign_method_signature.output.type_info.dotnet_type,
            method_name = method_name,
            dotnet_args = dotnet_args_str,
            dotnet_input_conversion = dotnet_input_conversion,
            maybe_return_bind = maybe_return_bind,
            full_method_name = full_method_name,
            pinvoke_call_args = pinvoke_call_args,
            maybe_dotnet_output_conversion = maybe_dotnet_output_conversion,
            maybe_return = maybe_return,
        )
        .with_note("Write to memory failed")?;

        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        for (_, cs_code) in self.additional_cs_code_for_types.drain() {
            write!(self.cs_file, "{}", cs_code)?;
        }
        writeln!(self.cs_file, "}} // namespace",)?;
        Ok(())
    }
}

impl LanguageGenerator for DotNetConfig {
    fn expand_items(
        &self,
        conv_map: &mut TypeMap,
        _target_pointer_width: usize,
        _code: &[SourceCode],
        items: Vec<ItemToExpand>,
        _remove_not_generated_files: bool,
    ) -> Result<Vec<TokenStream>> {
        DotNetGenerator::new(&self, conv_map)?.generate(items)
    }
}
