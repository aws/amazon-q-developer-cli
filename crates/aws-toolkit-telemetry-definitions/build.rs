use convert_case::{
    Case,
    Casing,
};
use quote::{
    format_ident,
    quote,
};

const DEF: &str = include_str!("./def.json");

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeDef {
    name: String,
    r#type: Option<String>,
    allowed_values: Option<Vec<String>>,
    description: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MetricDef {
    name: String,
    description: String,
    metadata: Option<Vec<MetricMetadata>>,
    passive: Option<bool>,
    unit: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MetricMetadata {
    r#type: String,
    required: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Def {
    types: Vec<TypeDef>,
    metrics: Vec<MetricDef>,
}

fn main() {
    println!("cargo:rerun-if-changed=def.json");

    let outdir = std::env::var("OUT_DIR").unwrap();

    let data = serde_json::from_str::<Def>(DEF).unwrap();

    let mut out = "
        #[allow(rustdoc::invalid_html_tags)]
        #[allow(rustdoc::bare_urls)]
        mod inner {
    "
    .to_string();

    out.push_str("pub mod types {");
    for t in data.types {
        let name = format_ident!("{}", t.name.to_case(Case::Pascal));

        let rust_type = match t.allowed_values {
            // enum
            Some(allowed_values) => {
                let mut variants = vec![];
                let mut variant_as_str = vec![];

                for v in allowed_values {
                    let ident = format_ident!("{}", v.replace('.', "").to_case(Case::Pascal));
                    variants.push(quote!(
                        #[doc = concat!("`", #v, "`")]
                        #ident
                    ));
                    variant_as_str.push(quote!(
                        #name::#ident => #v
                    ));
                }

                let description = t.description;

                quote::quote!(
                    #[doc = #description]
                    #[derive(Debug, Clone, PartialEq)]
                    #[non_exhaustive]
                    pub enum #name {
                        #(
                            #variants,
                        )*
                    }

                    impl #name {
                        pub fn as_str(&self) -> &'static str {
                            match self {
                                #( #variant_as_str, )*
                            }
                        }
                    }

                    impl ::std::fmt::Display for #name {
                        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                            f.write_str(self.as_str())
                        }
                    }
                )
                .to_string()
            },
            // struct
            None => {
                let r#type = match t.r#type.as_deref() {
                    Some("string") | None => quote!(::std::string::String),
                    Some("int") => quote!(::std::primitive::i64),
                    Some("double") => quote!(::std::primitive::f64),
                    Some("boolean") => quote!(::std::primitive::bool),
                    Some(other) => panic!("{}", other),
                };
                let description = t.description;

                quote::quote!(
                    #[doc = #description]
                    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
                    #[serde(transparent)]
                    pub struct #name(pub #r#type);

                    impl #name {
                        pub fn new(t: #r#type) -> Self {
                            Self(t)
                        }

                        pub fn value(&self) -> &#r#type {
                            &self.0
                        }

                        pub fn into_value(self) -> #r#type {
                            self.0
                        }
                    }

                    impl ::std::fmt::Display for #name {
                        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                            write!(f, "{}", self.0)
                        }
                    }

                    impl From<#r#type> for #name {
                        fn from(t: #r#type) -> Self {
                            Self(t)
                        }
                    }
                )
                .to_string()
            },
        };

        out.push_str(&rust_type);
    }
    out.push('}');

    out.push_str("pub mod metrics {");
    for m in data.metrics.clone() {
        let raw_name = m.name;
        let name = format_ident!("{}", raw_name.to_case(Case::Pascal));
        let description = m.description;

        let passive = m.passive.unwrap_or_default();

        let unit = match m.unit.map(|u| u.to_lowercase()).as_deref() {
            Some("bytes") => quote!(::amzn_toolkit_telemetry::types::Unit::Bytes),
            Some("count") => quote!(::amzn_toolkit_telemetry::types::Unit::Count),
            Some("milliseconds") => quote!(::amzn_toolkit_telemetry::types::Unit::Milliseconds),
            Some("percent") => quote!(::amzn_toolkit_telemetry::types::Unit::Percent),
            Some("none") | None => quote!(::amzn_toolkit_telemetry::types::Unit::None),
            Some(unknown) => {
                panic!("unknown unit: {:?}", unknown);
            },
        };

        let metadata = m.metadata.unwrap_or_default();

        let mut fields = Vec::new();
        for field in &metadata {
            let field_name = format_ident!("{}", &field.r#type.to_case(Case::Snake));
            let ty_name = format_ident!("{}", field.r#type.to_case(Case::Pascal));
            let ty = if field.required.unwrap_or_default() {
                quote!(crate::types::#ty_name)
            } else {
                quote!(::std::option::Option<crate::types::#ty_name>)
            };

            fields.push(quote!(
                #field_name: #ty
            ));
        }

        let metadata_entries = metadata.iter().map(|m| {
            let raw_name = &m.r#type;
            let key = format_ident!("{}", m.r#type.to_case(Case::Snake));

            let value = if m.required.unwrap_or_default() {
                quote!(.value(self.#key.to_string()))
            } else {
                quote!(.value(self.#key.map(|v| v.to_string()).unwrap_or_default()))
            };

            quote!(
                ::amzn_toolkit_telemetry::types::MetadataEntry::builder()
                    .key(#raw_name)
                    #value
                    .build()
            )
        });

        let rust_type = quote::quote!(
            #[doc = #description]
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            pub struct #name {
                /// The time that the event took place,
                pub create_time: ::std::option::Option<::std::time::SystemTime>,
                /// Value based on unit and call type,
                pub value: ::std::option::Option<f64>,
                #( pub #fields, )*
            }

            impl #name {
                const NAME: &'static ::std::primitive::str = #raw_name;
                const PASSIVE: ::std::primitive::bool = #passive;
                const UNIT: ::amzn_toolkit_telemetry::types::Unit = #unit;
            }

            impl crate::IntoMetricDatum for #name {
                fn into_metric_datum(self) -> ::amzn_toolkit_telemetry::types::MetricDatum {
                    let metadata_entries = vec![
                        #(
                            #metadata_entries,
                        )*
                    ];

                    let epoch_timestamp = self.create_time
                        .map_or_else(
                            || ::std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as ::std::primitive::i64,
                            |t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as ::std::primitive::i64
                        );

                    ::amzn_toolkit_telemetry::types::MetricDatum::builder()
                        .metric_name(#name::NAME)
                        .passive(#name::PASSIVE)
                        .unit(#name::UNIT)
                        .epoch_timestamp(epoch_timestamp)
                        .value(self.value.unwrap_or(1.0))
                        .set_metadata(Some(metadata_entries))
                        .build()
                        .unwrap()
               }
            }
        );

        out.push_str(&rust_type.to_string());
    }
    out.push('}');

    // enum of all metrics
    let mut metrics = Vec::new();
    for m in data.metrics {
        let name = format_ident!("{}", m.name.to_case(Case::Pascal));
        metrics.push(quote!(
            #name
        ));
    }
    out.push_str("#[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]\n#[serde(tag = \"type\", content = \"content\")]\npub enum Metric {\n");
    for m in metrics {
        out.push_str(&format!("{m}(crate::metrics::{m}),\n"));
    }
    out.push('}');

    out.push_str("}\npub use inner::*;");

    let file: syn::File = syn::parse_str(&out).unwrap();
    let pp = prettyplease::unparse(&file);

    // write an empty file to the output directory
    std::fs::write(format!("{}/mod.rs", outdir), pp).unwrap();
}