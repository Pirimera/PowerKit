//! Export and import documents, including WinGet import files.

use super::*;

/// WinGet source details that `winget import` expects for the two WinGet catalogs.
fn winget_source_details(source: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match source {
        "winget" => Some((
            "https://cdn.winget.microsoft.com/cache",
            "Microsoft.Winget.Source_8wekyb3d8bbwe",
            "Microsoft.PreIndexed.Package",
        )),
        "msstore" => Some((
            "https://storeedgefd.dsx.mp.microsoft.com/v9.0",
            "StoreEdgeFD",
            "Microsoft.Rest",
        )),
        _ => None,
    }
}

/// A `winget export`-shaped document; packages with options or from other
/// managers are repeated under a top-level `PowerKit` object.
pub fn export_document(packages: &[ExportPackage], creation_date: &str) -> String {
    let mut sources = String::new();
    for source in ["winget", "msstore"] {
        let Some((argument, identifier, kind)) = winget_source_details(source) else {
            continue;
        };
        let ids: Vec<String> = packages
            .iter()
            .filter(|package| {
                package.key.manager == ManagerKind::WinGet && package.key.source == source
            })
            .map(|package| {
                format!(
                    "        {{ \"PackageIdentifier\": {} }}",
                    json::quote(&package.key.id)
                )
            })
            .collect();
        if ids.is_empty() {
            continue;
        }
        if !sources.is_empty() {
            sources.push_str(",\n");
        }
        sources.push_str(&format!(
            "    {{\n      \"Packages\": [\n{}\n      ],\n      \"SourceDetails\": {{\n        \"Argument\": {},\n        \"Identifier\": {},\n        \"Name\": {},\n        \"Type\": {}\n      }}\n    }}",
            ids.join(",\n"),
            json::quote(argument),
            json::quote(identifier),
            json::quote(source),
            json::quote(kind)
        ));
    }
    let extras = ExportExtras {
        schema_version: ExportExtras::SCHEMA_VERSION,
        packages: packages
            .iter()
            .filter(|package| {
                !package.options.is_empty()
                    || package.key.manager != ManagerKind::WinGet
                    || winget_source_details(&package.key.source).is_none()
            })
            .cloned()
            .collect(),
    };
    format!(
        "{{\n  \"$schema\": \"https://aka.ms/winget-packages.schema.2.0.json\",\n  \"CreationDate\": {},\n  \"Sources\": [\n{sources}\n  ],\n  \"PowerKit\": {}\n}}\n",
        json::quote(creation_date),
        extras.to_json_object().replace('\n', "\n  ")
    )
}

/// Packages from a WinGet export, merged with options from a `PowerKit` object.
/// Returns `None` when the document has no `Sources` array.
pub fn parse_import(text: &str) -> Option<Vec<ExportPackage>> {
    let root = json::parse(text)?;
    let sources = root.get("Sources")?;
    sources.as_array()?;
    let mut packages: Vec<ExportPackage> = Vec::new();
    for source in sources.objects() {
        let Some(name) = source
            .get("SourceDetails")
            .and_then(|details| details.str_field("Name"))
        else {
            continue;
        };
        let Some(list) = source
            .get("Packages")
            .filter(|list| list.as_array().is_some())
        else {
            continue;
        };
        for package in list.objects() {
            let Some(id) = package.str_field("PackageIdentifier") else {
                continue;
            };
            let key = PackageKey::new(ManagerKind::WinGet, name.clone(), id);
            if packages.iter().any(|known| known.key == key) {
                continue;
            }
            packages.push(ExportPackage {
                key,
                options: InstallOptions {
                    version: package.str_field("Version"),
                    ..InstallOptions::default()
                },
            });
        }
    }
    let extras = root.get("PowerKit").and_then(ExportExtras::from_value);
    for extra in extras.map(|extras| extras.packages).unwrap_or_default() {
        match packages.iter_mut().find(|known| known.key == extra.key) {
            Some(known) => known.options = extra.options,
            None => packages.push(extra),
        }
    }
    Some(packages)
}

/// Top-level `PowerKit` object stored beside a WinGet export document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportExtras {
    pub schema_version: u32,
    pub packages: Vec<ExportPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPackage {
    pub key: PackageKey,
    pub options: InstallOptions,
}

impl ExportExtras {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn to_json_object(&self) -> String {
        let mut body = String::from("{\n  \"SchemaVersion\": ");
        body.push_str(&self.schema_version.to_string());
        body.push_str(",\n  \"Packages\": [");
        for (index, package) in self.packages.iter().enumerate() {
            if index > 0 {
                body.push(',');
            }
            body.push_str("\n    ");
            body.push_str(&package_to_json(package));
        }
        if !self.packages.is_empty() {
            body.push_str("\n  ");
        }
        body.push_str("]\n}");
        body
    }

    pub fn parse_json_object(text: &str) -> Option<Self> {
        Self::from_value(&json::parse(text)?)
    }

    fn from_value(value: &json::Value) -> Option<Self> {
        value.entries()?;
        let schema = value.get("SchemaVersion")?.as_u32()?;
        if schema != Self::SCHEMA_VERSION {
            return None;
        }
        let list = value.get("Packages")?;
        list.as_array()?;
        let mut packages = Vec::new();
        for object in list.objects() {
            packages.push(parse_export_package(object)?);
        }
        Some(Self {
            schema_version: schema,
            packages,
        })
    }
}

fn package_to_json(package: &ExportPackage) -> String {
    let mut body = format!(
        "{{\"Manager\":{},\"Source\":{},\"Id\":{}",
        json::quote(package.key.manager.as_str()),
        json::quote(&package.key.source),
        json::quote(&package.key.id)
    );
    if let Some(scope) = &package.options.scope {
        body.push_str(",\"Scope\":");
        body.push_str(&json::quote(scope));
    }
    if let Some(architecture) = &package.options.architecture {
        body.push_str(",\"Architecture\":");
        body.push_str(&json::quote(architecture));
    }
    if let Some(version) = &package.options.version {
        body.push_str(",\"Version\":");
        body.push_str(&json::quote(version));
    }
    if let Some(extra) = &package.options.extra_args {
        body.push_str(",\"ExtraArgs\":");
        body.push_str(&json::quote(extra));
    }
    body.push('}');
    body
}

fn parse_export_package(object: &json::Value) -> Option<ExportPackage> {
    let manager = ManagerKind::parse(&object.str_field("Manager")?)?;
    let source = object.str_field("Source")?;
    let id = object.str_field("Id")?;
    Some(ExportPackage {
        key: PackageKey::new(manager, source, id),
        options: InstallOptions {
            scope: object.str_field("Scope"),
            architecture: object.str_field("Architecture"),
            version: object.str_field("Version"),
            extra_args: object.str_field("ExtraArgs"),
        },
    })
}
