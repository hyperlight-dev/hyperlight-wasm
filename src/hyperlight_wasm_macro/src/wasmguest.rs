/*
Copyright 2024 The Hyperlight Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

// general todos:
// - split out the general guest codegen (to do an `impl Imports for
//   Host {}`) vs the wasmtime-specific codegen
// - once that is done it will be easy to support resources exported
//    from the guest properly. (the current issue is that since the
//    host-interaction code is fused with the wasmtime-interface code,
//    it is impossible to come up with an <I: Imports> to instantiate
//    the `Resources` struct with.)

use hyperlight_component_util::emit::{
    FnName, State, WitName, kebab_to_fn, kebab_to_namespace, kebab_to_type, kebab_to_var,
    split_wit_name,
};
use hyperlight_component_util::etypes::{
    self, Component, Defined, ExternDecl, ExternDesc, Handleable, Instance, Tyvar,
};
use hyperlight_component_util::hl::{
    emit_fn_hl_name, emit_hl_marshal_param, emit_hl_marshal_result, emit_hl_unmarshal_param,
    emit_hl_unmarshal_result,
};
use hyperlight_component_util::{resource, rtypes};
use proc_macro::Ident;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt;

// Emit code to register this particular extern definition with the
// wasmtime linker, calling through Hyperlight.
//
// depth: how many instances deep (from the root component) this is,
// used to keep track of which linker instance to register on
fn emit_import_extern_decl<'a, 'b, 'c>(
    s: &'c mut State<'a, 'b>,
    depth: u32,
    ed: &'c ExternDecl<'b>,
) -> TokenStream {
    match &ed.desc {
        ExternDesc::CoreModule(_) => panic!("core module (im/ex)ports are not supported"),
        ExternDesc::Func(ft) => {
            let fname = emit_fn_hl_name(s, ed.kebab_name);
            let li = format_ident!("li{}", depth);
            let edkn = ed.kebab_name;
            let pts = ft
                .params
                .iter()
                .map(|p| rtypes::emit_value(s, &p.ty))
                .collect::<Vec<_>>();
            let (pds, pus) = ft.params.iter()
                .map(|p| {
                    let id = kebab_to_var(p.name.name);
                    let pd = quote! { #id };
                    let pu = emit_hl_marshal_param(s, id, &p.ty);
                    (pd, quote! { ::hyperlight_common::flatbuffer_wrappers::function_types::ParameterValue::VecBytes(#pu) })
                })
                .unzip::<_, _, Vec<_>, Vec<_>>();
            let ret = format_ident!("ret");
            let is_ret_empty = match &ft.result {
                etypes::Result::Named(rs) if rs.len() == 0 => true,
                _ => false,
            };
            let ur = if is_ret_empty {
                quote! { () }
            } else {
                let ur = emit_hl_unmarshal_result(s, ret.clone(), &ft.result);
                quote! { ({ #ur },) }
            };
            let rt = if is_ret_empty {
                quote! { () }
            } else {
                let rt = rtypes::emit_func_result(s, &ft.result);
                quote! { (#rt,) }
            };
            quote! {
                #li.func_wrap::<_, (#(#pts,)*), #rt>(#edkn, |_, (#(#pds,)*)| {
                    let #ret = call_host_function::<Vec<u8>>(
                        #fname,
                        ::core::option::Option::Some(vec![#(#pus,)*]),
                        ::hyperlight_common::flatbuffer_wrappers::function_types::ReturnType::VecBytes,
                    ).unwrap();
                    ::core::result::Result::Ok(#ur)
                }).unwrap();
            }
        }
        ExternDesc::Type(t) => match t {
            Defined::Handleable(Handleable::Var(Tyvar::Bound(b))) => {
                let (b, _) = s.resolve_tv(*b);
                let li = format_ident!("li{}", depth);
                let edkn = ed.kebab_name;
                let rtid = format_ident!("HostResource{}", b);
                quote! {
                    #li.resource(#edkn, ::wasmtime::component::ResourceType::host::<#rtid>(), |_, _| { Ok(()) });
                }
            }
            _ => quote! {},
        },
        ExternDesc::Instance(it) => {
            let edkn = ed.kebab_name;
            let wn = split_wit_name(ed.kebab_name);
            let li = format_ident!("li{}", depth);
            let depth = depth + 1;
            let lin = format_ident!("li{}", depth);
            let mut ret = quote! {
                let mut #lin = #li.instance(#edkn).unwrap();
            };
            ret.extend(emit_import_instance(s, wn.clone(), depth, it));
            ret
        }
        ExternDesc::Component(_) => {
            panic!("nested components not yet supported in rust bindings");
        }
    }
}

// Emit code to register this particular extern definition with
// Hyperlight as a callable function.
//
// path: the instance path (from the root component) where this
// definition may be found, used to locate the wasmtime function to
// call.
fn emit_export_extern_decl<'a, 'b, 'c>(
    s: &'c mut State<'a, 'b>,
    path: Vec<String>,
    ed: &'c ExternDecl<'b>,
) -> TokenStream {
    match &ed.desc {
        ExternDesc::CoreModule(_) => panic!("core module (im/ex)ports are not supported"),
        ExternDesc::Func(ft) => {
            let fname = emit_fn_hl_name(s, ed.kebab_name);
            let n = match kebab_to_fn(ed.kebab_name) {
                FnName::Plain(n) => n,
                FnName::Associated(_, _) => {
                    panic!("resources exported from wasm not yet supported")
                }
            };
            let nlit = ed.kebab_name;
            let pts = ft.params.iter().map(|_| quote! { ::hyperlight_common::flatbuffer_wrappers::function_types::ParameterType::VecBytes }).collect::<Vec<_>>();
            let pwts = ft
                .params
                .iter()
                .map(|p| rtypes::emit_value(s, &p.ty))
                .collect::<Vec<_>>();
            let (pds, pus) = ft.params.iter().enumerate()
                .map(|(i, p)| {
                    let id = kebab_to_var(p.name.name);
                    let pd = quote! { let ::hyperlight_common::flatbuffer_wrappers::function_types::ParameterValue::VecBytes(#id) = &fc.parameters.as_ref().unwrap()[#i] else { panic!("invariant violation: host passed non-VecBytes core hyperlight argument"); }; };
                    let pu = emit_hl_unmarshal_param(s, id, &p.ty);
                    (pd, pu)
                })
                .unzip::<_, _, Vec<_>, Vec<_>>();
            let get_instance = path.iter().map(|export| quote! {
                let instance_idx = Some(instance.get_export_index(&mut *store, instance_idx.as_ref(), #export).unwrap());
            }).collect::<Vec<_>>();
            let (function_call, ret) = emit_wasm_function_call(s, &ft.result, pwts, pus);
            let marshal_result = emit_hl_marshal_result(s, ret.clone(), &ft.result);
            quote! {
                fn #n(fc: &::hyperlight_common::flatbuffer_wrappers::function_call::FunctionCall) -> ::hyperlight_guest::error::Result<::alloc::vec::Vec<u8>> {
                    #(#pds)*
                    let mut store = CUR_STORE.lock(); let mut store = store.as_mut().unwrap();
                    let instance = CUR_INSTANCE.lock(); let mut instance = instance.unwrap();
                    let instance_idx = None;
                    #(#get_instance;)*
                    let func_idx = instance.get_export_index(&mut *store, instance_idx.as_ref(), #nlit).unwrap();
                    #function_call
                    ::core::result::Result::Ok(::hyperlight_common::flatbuffer_wrappers::util::get_flatbuffer_result::<&[u8]>(&#marshal_result))
                }
                ::hyperlight_guest_bin::guest_function::register::register_function(
                    ::hyperlight_guest_bin::guest_function::definition::GuestFunctionDefinition::new(
                        #fname.to_string(),
                        ::alloc::vec![#(#pts),*],
                        ::hyperlight_common::flatbuffer_wrappers::function_types::ReturnType::VecBytes,
                        #n as usize
                    )
                );
            }
        }
        ExternDesc::Type(_) => {
            // no runtime representation is needed for types
            quote! {}
        }
        ExternDesc::Instance(it) => {
            let wn = split_wit_name(ed.kebab_name);
            let mut path = path.clone();
            path.push(ed.kebab_name.to_string());
            emit_export_instance(s, wn.clone(), path, it)
        }
        ExternDesc::Component(_) => {
            panic!("nested components not yet supported in rust bindings");
        }
    }
}

fn emit_wasm_function_call(
    s: &mut State,
    result: &etypes::Result,
    pwts: Vec<TokenStream>,
    pus: Vec<TokenStream>,
) -> (TokenStream, proc_macro2::Ident) {
    let ret = format_ident!("ret");

    // if the result is empty we don't want a return result with `get_typed_func`
    let rwt = match result {
        etypes::Result::Named(rs) if rs.is_empty() => {
            quote! {
                instance.get_typed_func::<(#(#pwts,)*), ()>(&mut *store, func_idx)?
                    .call(&mut *store, (#(#pus,)*))?;
            }
        }
        _ => {
            let r = rtypes::emit_func_result(s, result);
            quote! {
                let #ret = instance.get_typed_func::<(#(#pwts,)*), ((#r,))>(&mut *store, func_idx)?
                    .call(&mut *store, (#(#pus,)*))?.0;
            }
        }
    };

    (rwt, ret)
}

// Emit code to register each export of the given instance with the
// wasmtime linker, calling through Hyperlight.
//
// depth: how many instances deep (from the root component) this is,
// used to keep track of which linker instance to register on
fn emit_import_instance<'a, 'b, 'c>(
    s: &'c mut State<'a, 'b>,
    wn: WitName,
    depth: u32,
    it: &'c Instance<'b>,
) -> TokenStream {
    let mut s = s.with_cursor(wn.namespace_idents());
    s.cur_helper_mod = Some(kebab_to_namespace(wn.name));
    s.cur_trait = Some(kebab_to_type(wn.name));
    let imports = it
        .exports
        .iter()
        .map(|ed| emit_import_extern_decl(&mut s, depth, ed))
        .collect::<Vec<_>>();
    quote! { #(#imports)* }
}

// Emit code to register each export of the given instance with
// Hyperlight as a callable function.
//
// path: the instance path (from the root component) where this
// definition may be found, used to locate the wasmtime function to
// call.
fn emit_export_instance<'a, 'b, 'c>(
    s: &'c mut State<'a, 'b>,
    wn: WitName,
    path: Vec<String>,
    it: &'c Instance<'b>,
) -> TokenStream {
    let mut s = s.with_cursor(wn.namespace_idents());
    s.cur_helper_mod = Some(kebab_to_namespace(wn.name));
    s.cur_trait = Some(kebab_to_type(wn.name));
    let exports = it
        .exports
        .iter()
        .map(|ed| emit_export_extern_decl(&mut s, path.clone(), ed))
        .collect::<Vec<_>>();
    quote! { #(#exports)* }
}

// Emit:
// - a resource table for all resource exported by this component, to
//   keep track of resources sent to the host
// - code to register each import with the wasmtime linker
// - code to register each export with Hyperlight
fn emit_component<'a, 'b, 'c>(
    s: &'c mut State<'a, 'b>,
    wn: WitName,
    ct: &'c Component<'b>,
) -> TokenStream {
    let mut s = s.with_cursor(wn.namespace_idents());
    let ns = wn.namespace_path();
    let r#trait = kebab_to_type(wn.name);
    let import_trait = format_ident!("{}Imports", r#trait);
    let export_trait = format_ident!("{}Exports", r#trait);
    s.import_param_var = Some(format_ident!("I"));
    s.self_param_var = Some(format_ident!("S"));

    resource::emit_tables(
        &mut s,
        format_ident!("{}Resources", kebab_to_type(wn.name)),
        quote! { #ns::#import_trait + ::core::marker::Send + 'static },
        Some(quote! { #ns::#export_trait<I> }),
        true,
    );
    s.root_mod
        .items
        .extend(s.bound_vars.iter().enumerate().map(|(i, _)| {
            let id = format_ident!("HostResource{}", i);
            quote! {
                // this doesn't actually need to be Lift/Lower, but
                // unfortunately the derive macros on the other structs
                // don't (can't? due to lack of type information) properly
                // realise that
                #[derive(::wasmtime::component::ComponentType)]
                #[derive(::wasmtime::component::Lift)]
                #[derive(::wasmtime::component::Lower)]
                #[component(record)]
                struct #id { rep: u32 }
            }
        }));

    s.var_offset = ct.instance.evars.len();
    let imports = ct
        .imports
        .iter()
        .map(|ed| emit_import_extern_decl(&mut s, 0, ed))
        .collect::<Vec<_>>();
    s.var_offset = 0;

    let exports = ct
        .instance
        .unqualified
        .exports
        .iter()
        .map(|ed| emit_export_extern_decl(&mut s, Vec::new(), ed))
        .collect::<Vec<_>>();

    quote! {
        let mut linker = CUR_LINKER.lock(); let mut linker = linker.as_mut().unwrap();
        let mut li0 = linker.root();
        #(#imports)*
        #(#exports)*
    }
}

pub fn emit_toplevel<'a, 'b, 'c>(s: &'c mut State<'a, 'b>, n: &str, ct: &'c Component<'b>) {
    s.is_impl = true;
    let wn = split_wit_name(n);
    let tokens = emit_component(s, wn, ct);
    s.root_mod.items.extend(quote! {
        fn hyperlight_guest_wasm_init() {
            #tokens
        }
    });
}
