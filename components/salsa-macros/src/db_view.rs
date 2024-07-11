use std::fmt::Display;

use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};

use crate::hygiene::Hygiene;

// Source:
//
// ```
// #[salsa::db_view]
// pub trait $Db: ... {
//     ...
// }
// ```
//
// becomes
//
// ```
// pub trait $Db: __SalsaViewAs$Db__ {
//     ...
// }
//
// pub trait __SalsaViewAs$Db__ {
//     fn __salsa_add_view_for_$db__(&self);
// }
//
// impl<T: Db> __SalsaViewAs$Db__ for T {
//     ...
// }
// ```
pub(crate) fn db_view(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    syn::parse_macro_input!(args as syn::parse::Nothing);
    let db_view_macro = DbViewMacro::new(
        Hygiene::from(&input),
        syn::parse_macro_input!(input as syn::ItemTrait),
    );

    match db_view_macro.expand() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[allow(non_snake_case)]
pub(crate) struct DbViewMacro {
    hygiene: Hygiene,
    input: syn::ItemTrait,
    DbViewTrait: syn::Ident,
    db_view_method: syn::Ident,
}

#[allow(non_snake_case)]
impl DbViewMacro {
    // This is a case where our hygiene mechanism is inadequate.
    //
    // We cannot know whether `DbViewTrait` is defined elsewhere
    // in the module.
    //
    // Therefore we give it a dorky name.

    pub(crate) fn db_view_trait_name(input: &impl Display) -> syn::Ident {
        syn::Ident::new(&format!("__SalsaAddView{}__", input), Span::call_site())
    }

    pub(crate) fn db_view_method_name(input: &impl Display) -> syn::Ident {
        syn::Ident::new(
            &format!("__salsa_add_view_{}__", input.to_string().to_snake_case()),
            Span::call_site(),
        )
    }

    fn new(hygiene: Hygiene, input: syn::ItemTrait) -> Self {
        Self {
            DbViewTrait: Self::db_view_trait_name(&input.ident),
            db_view_method: Self::db_view_method_name(&input.ident),
            hygiene,
            input,
        }
    }

    fn expand(mut self) -> syn::Result<TokenStream> {
        self.add_supertrait();
        let view_impl = self.view_impl();
        let view_trait = self.view_trait();

        let input = self.input;
        Ok(quote! {
            #input
            #view_trait
            #view_impl
        })
    }

    fn add_supertrait(&mut self) {
        let Self { DbViewTrait, .. } = self;
        self.input.supertraits.push(parse_quote! { #DbViewTrait })
    }

    fn view_trait(&self) -> syn::ItemTrait {
        let Self {
            DbViewTrait,
            db_view_method,
            ..
        } = self;

        let vis = &self.input.vis;
        parse_quote! {
            /// Internal salsa method generated by the `salsa::db_view` macro
            /// that registers this database view trait with the salsa database.
            ///
            /// Nothing to see here.
            #[doc(hidden)]
            #vis trait #DbViewTrait {
                fn #db_view_method(&self);
            }
        }
    }

    fn view_impl(&self) -> syn::Item {
        let Self {
            DbViewTrait,
            db_view_method,
            ..
        } = self;

        let DB = self.hygiene.ident("DB");
        let Database = self.hygiene.ident("Database");
        let views = self.hygiene.ident("views");
        let UserTrait = &self.input.ident;

        parse_quote! {
            const _: () = {
                use salsa::Database as #Database;

                #[doc(hidden)]
                impl<#DB: #Database> #DbViewTrait for #DB {
                    /// Internal salsa method generated by the `salsa::db_view` macro
                    /// that registers this database view trait with the salsa database.
                    ///
                    /// Nothing to see here.
                    fn #db_view_method(&self) {
                        let #views = self.views_of_self();
                        #views.add::<dyn #UserTrait>(|t| t, |t| t);
                    }
                }
            };
        }
    }
}