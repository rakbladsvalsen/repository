use convert_case::{Case, Casing};
use darling::*;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_error::{abort, proc_macro_error};
use quote::quote;
use syn::{
    parse_macro_input, spanned::Spanned, AngleBracketedGenericArguments, DataStruct, DeriveInput,
    Expr, Ident, PathArguments,
};

#[derive(FromAttributes, Default, Debug)]
#[darling(attributes(as_query))]
struct AsQueryStructOptions {
    sort_default_column: Option<String>,
    camel_case: Option<bool>,
}

#[derive(FromAttributes, Default, Debug)]
#[darling(attributes(as_query))]
struct AsQueryInnerFieldOptions {
    column: Option<String>,
    lte: Option<bool>,
    gte: Option<bool>,
    eq: Option<bool>,
    lt: Option<bool>,
    gt: Option<bool>,
    like: Option<bool>,
    contains: Option<bool>,
    custom_convert: Option<String>,
}

impl AsQueryInnerFieldOptions {
    fn filters_as_list(&self) -> Vec<String> {
        let mut ret = vec![];
        if self.lte.is_some() {
            ret.push("lte".into());
        }
        if self.gte.is_some() {
            ret.push("gte".into());
        }
        if self.eq.is_some() {
            ret.push("eq".into());
        }
        if self.lt.is_some() {
            ret.push("lt".into());
        }
        if self.gt.is_some() {
            ret.push("gt".into());
        }
        if self.like.is_some() {
            ret.push("like".into());
        }
        if self.contains.is_some() {
            ret.push("contains".into());
        }
        ret
    }
}

#[derive(Debug)]
struct PathSegmentAnalyze {
    is_option: bool,
    _inner_type: String,
}

impl PathSegmentAnalyze {
    pub fn new(ty: &syn::Type) -> syn::Result<Self> {
        if let syn::Type::Path(ty_path) = ty {
            let last_segment = ty_path.path.segments.last().ok_or_else(|| {
                syn::Error::new(ty_path.span(), "Couldn't determine the type of this field")
            })?;
            if let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) =
                &last_segment.arguments
            {
                if let Some(syn::GenericArgument::Type(syn::Type::Path(syn::TypePath {
                    path,
                    ..
                }))) = args.last()
                {
                    let last_segment = path.segments.last().ok_or_else(|| {
                        syn::Error::new(last_segment.span(), "Couldn't extract inner type")
                    })?;
                    return Ok(PathSegmentAnalyze {
                        is_option: last_segment.ident == "Option",
                        _inner_type: last_segment.ident.to_string(),
                    });
                }
            }
            Ok(PathSegmentAnalyze {
                is_option: last_segment.ident == "Option",
                _inner_type: last_segment.ident.to_string(),
            })
        } else {
            Err(syn::Error::new(ty.span(), "Couldn't extract type"))
        }
    }
}

fn expand(ast: DeriveInput) -> syn::Result<TokenStream2> {
    let struct_name = &ast.ident;
    let struct_name_optionized = format!("{struct_name}AsQuery");
    let bident = Ident::new(&struct_name_optionized, struct_name.span());
    // extract this struct's fields
    let fields = if let syn::Data::Struct(DataStruct {
        fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
        ..
    }) = ast.data
    {
        named
    } else {
        abort!(struct_name, "This macro may only be used with structs.");
    };
    let mut available_filtering_columns = vec![];
    // the attributes for this struct (not its fields).
    let struct_options = AsQueryStructOptions::from_attributes(&ast.attrs)?;
    let mut sort_expr = None;
    // whether sorting feature is enabled or not. if the struct doesn't have the
    // "sort_default_column" attribute, then this feature will be disabled.
    let sorting_enabled = struct_options.sort_default_column.as_ref().is_some();

    let mut optionized_fields = vec![];
    let mut filter_fn_matches = vec![];

    for field in fields {
        let (field_ident, ty) = (&field.ident, &field.ty);
        if field_ident.is_none() {
            continue;
        }
        let field_ident = field_ident.as_ref().ok_or(syn::Error::new(
            field_ident.span(),
            "Couldn't extract identifier",
        ))?;
        let attr2 = AsQueryInnerFieldOptions::from_attributes(&field.attrs)?;
        if attr2.column.as_ref().is_none() {
            // this field has no "column" attribute.
            continue;
        }

        let path_analyzed = PathSegmentAnalyze::new(ty)?;
        let column = attr2
            .column
            .clone()
            .ok_or_else(|| syn::Error::new(field.span(), "Missing 'column=blah' attribute."))?;
        let db_column = syn::parse_str::<Expr>(&column)?;

        for f in attr2.filters_as_list() {
            let new_field_name = format!("{field_ident}_{}", f);
            let field_ident = Ident::new(&new_field_name, field_ident.span());
            let value = match &attr2.custom_convert {
                Some(value) => syn::parse_str::<Expr>(value)?,
                _ => syn::parse_str::<Expr>("value")?,
            };

            let f = Ident::new(&f, field_ident.span());

            filter_fn_matches.push(quote! {
                select = match self.#field_ident.as_ref() {
                    Some(value) => select.filter(#db_column.#f(#value)),
                    _ => select,
                };
            });

            optionized_fields.push(if path_analyzed.is_option {
                quote! {
                    #field_ident: #ty
                }
            } else {
                quote! {
                    #field_ident: std::option::Option<#ty>
                }
            })
        }

        if sorting_enabled {
            let mut field_name_asc = format!("{field_ident}");
            // if camel case is enabled, then match against the camelCase-ized field
            // instead.
            if struct_options.camel_case.unwrap_or(false) {
                field_name_asc = field_name_asc.to_case(Case::Camel)
            }
            let field_name_desc = format!("-{field_name_asc}");
            // build "match =>" arms.
            available_filtering_columns.push(quote! {
                #field_name_asc => select.order_by_asc(#db_column),
                #field_name_desc => select.order_by_desc(#db_column),
            });
        }
    }

    if sorting_enabled {
        let default_sort_column =
            syn::parse_str::<Expr>(&struct_options.sort_default_column.ok_or(syn::Error::new(
                struct_name.span(),
                "Missing default sort column",
            ))?)?;
        sort_expr = Some(quote! {
            impl AsQueryParamSortable for #bident{
                fn sort<E: sea_orm::EntityTrait>(&self, mut select: Select<E>) -> Select<E> {
                    if self.order_by.as_ref().is_none(){
                        return select;
                    }
                    select = match self.order_by.as_ref().unwrap().as_str(){
                        #(#available_filtering_columns)*
                        _ => select.order_by_asc(#default_sort_column)
                    };
                    select
                }
            }
        })
    }

    let expanded = quote! {
        use sea_orm::QueryOrder;

        #[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
        #[serde(rename_all = "camelCase")]
        pub struct #bident {
            order_by: std::option::Option<String>,
            #(#optionized_fields,)*
        }

        #sort_expr


        impl AsQueryParamFilterable for #bident {

            fn filter<E: EntityTrait>(&self, mut select: Select<E>) -> Select<E> {
                #(#filter_fn_matches)*
                select
            }
        }
    };
    Ok(expanded)
}

#[proc_macro_derive(AsQueryParam, attributes(as_query))]
#[proc_macro_error]
pub fn derive(input: TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    expand(ast)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
