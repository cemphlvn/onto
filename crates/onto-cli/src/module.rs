//! Loading `.onto` files: several categories, functors, imports.
//!
//! `FILE` loads the first category the file itself declares;
//! `FILE#Name` picks a category by name (declared or imported). Imports
//! resolve relative to the importing file; a cycle is an error.

use std::path::{Path, PathBuf};

use onto_core::{Category, Error, Module};

use crate::run::BoxError;

/// Splits `file.onto#Name`.
pub fn target(spec: &Path) -> (PathBuf, Option<String>) {
    let s = spec.to_string_lossy();
    match s.rsplit_once('#') {
        Some((f, name)) if !name.is_empty() => (PathBuf::from(f), Some(name.to_owned())),
        _ => (spec.to_path_buf(), None),
    }
}

pub fn load_module(file: &Path) -> Result<(Module, String), BoxError> {
    let src = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let root = std::fs::canonicalize(file)?;
    // Who imported whom: a cycle is an import already on the importer's
    // chain; a file imported twice elsewhere (a shared standard) is loaded
    // once.
    let mut parent: std::collections::HashMap<PathBuf, PathBuf> = Default::default();
    let mut loaded: std::collections::HashSet<PathBuf> = [root.clone()].into();
    let module =
        onto_core::parse_module_at(&root.to_string_lossy(), &src, &mut |path, importer| {
            let importer = PathBuf::from(importer);
            let base = importer.parent().unwrap_or(Path::new("."));
            let full = std::fs::canonicalize(base.join(path)).map_err(|e| Error::Parse {
                line: 0,
                msg: format!("import \"{path}\": {e}"),
            })?;
            let mut at = Some(importer.clone());
            while let Some(p) = at {
                if p == full {
                    return Err(Error::Parse {
                        line: 0,
                        msg: format!("import cycle through {}", full.display()),
                    });
                }
                at = parent.get(&p).cloned();
            }
            let id = full.to_string_lossy().into_owned();
            if !loaded.insert(full.clone()) {
                return Ok((id, String::new()));
            }
            parent.insert(full.clone(), importer);
            let text = std::fs::read_to_string(&full).map_err(|e| Error::Parse {
                line: 0,
                msg: format!("{}: {e}", full.display()),
            })?;
            Ok((id, text))
        })?;
    Ok((module, onto_core::parse::snapshot_hash(&src)))
}

/// The category `spec` names (see the module docs).
pub fn load_category(spec: &Path) -> Result<Category, BoxError> {
    let (file, name) = target(spec);
    let (module, own) = load_module(&file)?;
    let found = match &name {
        Some(n) => module.category(n).cloned(),
        None => module
            .categories
            .iter()
            .find(|c| c.snapshot() == Some(own.as_str()))
            .cloned(),
    };
    found.ok_or_else(|| {
        let names: Vec<&str> = module.categories.iter().map(|c| c.name()).collect();
        match name {
            Some(n) => format!(
                "{}: no category `{n}` (has {})",
                file.display(),
                names.join(", ")
            ),
            None => format!("{}: declares no category", file.display()),
        }
        .into()
    })
}
