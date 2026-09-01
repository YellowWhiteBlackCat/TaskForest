//! source-inspection: static-policy
//!
//! Long-term renderer/data dependency boundary (ARCH.md §8.1).
//!
//! The folding-layer contract ("一次折叠，三端渲染") says render modules may
//! not fold typed observations into display semantics themselves — that fold
//! belongs to a pure data-layer module (page `*_vm`/`*_stats`/`projection`
//! builders or `taskmanager-shell`). The 2026-08-24 migration removed every
//! known exception in the same change as its replacement. This policy test has
//! no migration allowlist and is not evidence that a migration is complete.
//!
//! Since 2026-08-20 the gate covers the product frontends, each with
//! its own scan root and paint signature:
//!   * GPUI — `crates/taskmanager-gpui/src/gpui_app`;
//!   * Iced — `crates/taskmanager-iced/src`;
//!   * TUI  — `crates/taskmanager-tui/src`;
//!   * Bevy — `crates/taskmanager-bevy-ui/src`.
//!
//! Detection is deliberately fail-closed and syntax-aware — it flags the
//! observable signature of an inline fold in the exact function or trait
//! method that paints (`fn render`, `impl TableDelegate`, or a gpui `div()`
//! builder — per-frontend paint idioms below). Rust is parsed with `syn`, and
//! `ExprMethodCall` nodes are inspected individually. A non-observation call
//! in the same file or function can therefore never hide a real
//! `current_*` observation call.
//!
//! The rule matches the `current_*` accessor family by PREFIX, not by an
//! exhaustive suffix whitelist. A suffix list fails OPEN: accessors outside
//! the enumerated suffixes silently escape the gate. Prefix matching fails
//! CLOSED: an unknown `current_*` call in a paint function trips the gate.
//! A small method-level exception list covers accessors that return identity or
//! an already-folded aggregate (`current_value` / `current_number` and the
//! process identity accessors); the exception is evaluated per AST call, not
//! as a file-wide negative condition.
//!
//! Direct `*Availability::` pattern matches stay out by design: they fold
//! availability state, not scalar observation values, and matching enum
//! variants by prefix would be a different (and noisier) gate.
//!
//! A finding is fixed by moving the read into the page's data-layer module and
//! passing the folded string/ViewModel down. Adding an exception is not a fix.

use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use quote::quote;
use syn::visit::{self, Visit};
use syn::{Block, Expr, ExprCall, ExprMethodCall, ImplItem, Item, ItemImpl, Macro, Signature};

const GPUI_SCAN_ROOT: &str = "crates/taskmanager-gpui/src/gpui_app";
const ICED_SCAN_ROOT: &str = "crates/taskmanager-iced/src";
const TUI_SCAN_ROOT: &str = "crates/taskmanager-tui/src";
const BEVY_SCAN_ROOT: &str = "crates/taskmanager-bevy-ui/src";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Frontend {
    Gpui,
    Iced,
    Tui,
    Bevy,
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// `current_*` calls surveyed as NOT scalar observation reads. The exception
/// is intentionally method-level: it cannot suppress a different call in the
/// same function.
const NON_OBSERVATION_READ_METHODS: &[&str] = &[
    "current_value",
    "current_number",
    "current_user",
    "current_start_token",
    "current_exe_path",
];

#[derive(Default)]
struct ObservationReadFinder {
    found: bool,
}

impl<'ast> Visit<'ast> for ObservationReadFinder {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method = node.method.to_string();
        if method.starts_with("current_")
            && !NON_OBSERVATION_READ_METHODS.contains(&method.as_str())
        {
            self.found = true;
        }
        visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        if macro_tokens_have_observation_read(&node.tokens) {
            self.found = true;
        }
        visit::visit_macro(self, node);
    }
}

/// Inspect macro token trees as well as ordinary expressions. Declarative
/// scene macros can contain embedded Rust expressions that `syn` must retain
/// as an opaque token stream; ignoring them would leave a new renderer fold
/// hidden inside `bsn!` or another scene builder.
fn macro_tokens_have_observation_read(tokens: &TokenStream) -> bool {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    for (index, tree) in trees.iter().enumerate() {
        if matches!(tree, TokenTree::Punct(punct) if punct.as_char() == '.')
            && let Some(TokenTree::Ident(method)) = trees.get(index + 1)
        {
            let method = method.to_string();
            if method.starts_with("current_")
                && !NON_OBSERVATION_READ_METHODS.contains(&method.as_str())
            {
                return true;
            }
        }
        if let TokenTree::Group(group) = tree
            && macro_tokens_have_observation_read(&group.stream())
        {
            return true;
        }
    }
    false
}

/// Detect an observation call in one exact function body.
fn has_observation_read(block: &Block) -> bool {
    let mut finder = ObservationReadFinder::default();
    finder.visit_block(block);
    finder.found
}

#[derive(Default)]
struct CallFinder<'a> {
    method: Option<&'a str>,
    free_function: Option<&'a str>,
    found: bool,
}

impl<'ast> Visit<'ast> for CallFinder<'_> {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if self.method.is_some_and(|expected| node.method == expected) {
            self.found = true;
        }
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Some(expected) = self.free_function
            && let Expr::Path(path) = node.func.as_ref()
            && path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == expected)
        {
            self.found = true;
        }
        visit::visit_expr_call(self, node);
    }
}

fn has_method_call(block: &Block, method: &str) -> bool {
    let mut finder = CallFinder {
        method: Some(method),
        ..CallFinder::default()
    };
    finder.visit_block(block);
    finder.found
}

fn has_free_function_call(block: &Block, function: &str) -> bool {
    let mut finder = CallFinder {
        free_function: Some(function),
        ..CallFinder::default()
    };
    finder.visit_block(block);
    finder.found
}

fn signature_contains(signature: &Signature, needle: &str) -> bool {
    quote!(#signature).to_string().contains(needle)
}

/// Whether one exact function is a render/scene function for its frontend.
fn paints_function(frontend: Frontend, signature: &Signature, body: &Block) -> bool {
    let name = signature.ident.to_string();
    match frontend {
        Frontend::Gpui => {
            name.starts_with("render")
                || signature_contains(signature, "Div")
                || has_free_function_call(body, "div")
        }
        Frontend::Iced => {
            name == "view" || name.starts_with("render") || signature_contains(signature, "Element")
        }
        Frontend::Tui => {
            name.starts_with("render") || name == "draw" || signature_contains(signature, "Frame")
        }
        Frontend::Bevy => {
            name == "content"
                || name.starts_with("render")
                || name.starts_with("paint")
                || signature_contains(signature, "Scene")
                || has_method_call(body, "spawn_scene")
        }
    }
}

fn paints_impl(frontend: Frontend, item: &ItemImpl) -> bool {
    let Some((_, path, _)) = &item.trait_ else {
        return false;
    };
    let Some(name) = path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    else {
        return false;
    };
    match frontend {
        Frontend::Gpui => matches!(name.as_str(), "TableDelegate" | "Widget"),
        Frontend::Iced | Frontend::Tui => name == "Widget",
        Frontend::Bevy => name == "Scene",
    }
}

fn scan_items(items: &[Item], frontend: Frontend) -> bool {
    for item in items {
        match item {
            Item::Fn(function)
                if paints_function(frontend, &function.sig, &function.block)
                    && has_observation_read(&function.block) =>
            {
                return true;
            }
            Item::Impl(item_impl) => {
                let impl_paints = paints_impl(frontend, item_impl);
                for impl_item in &item_impl.items {
                    let ImplItem::Fn(function) = impl_item else {
                        continue;
                    };
                    if (impl_paints || paints_function(frontend, &function.sig, &function.block))
                        && has_observation_read(&function.block)
                    {
                        return true;
                    }
                }
            }
            Item::Mod(module) => {
                if let Some((_, nested)) = &module.content
                    && scan_items(nested, frontend)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn source_has_inline_observation_fold(
    source: &str,
    frontend: Frontend,
) -> Result<bool, syn::Error> {
    let file = syn::parse_file(source)?;
    Ok(scan_items(&file.items, frontend))
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn render_modules_gain_no_new_inline_observation_folds() {
    let repo = repository();
    let frontends: [(&str, Frontend); 4] = [
        (GPUI_SCAN_ROOT, Frontend::Gpui),
        (ICED_SCAN_ROOT, Frontend::Iced),
        (TUI_SCAN_ROOT, Frontend::Tui),
        (BEVY_SCAN_ROOT, Frontend::Bevy),
    ];

    let mut offenders = Vec::new();
    for (scan_root, frontend) in frontends {
        // A moved/renamed frontend crate must fail the gate, not skip it.
        let root_path = repo.join(scan_root);
        assert!(
            root_path.is_dir(),
            "scan root missing: {scan_root} — update the gate's frontend roots"
        );
        let mut files = Vec::new();
        if let Err(error) = collect_rs_files(&root_path, &mut files) {
            offenders.push(format!("{scan_root} (scan failed: {error})"));
            continue;
        }
        assert!(!files.is_empty(), "scan root empty: {scan_root}");

        for path in &files {
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(error) => {
                    let relative = path
                        .strip_prefix(&root_path)
                        .expect("scanned path is inside the scan root")
                        .to_string_lossy()
                        .replace('\\', "/");
                    offenders.push(format!("{scan_root}/{relative} (read failed: {error})"));
                    continue;
                }
            };
            match source_has_inline_observation_fold(&source, frontend) {
                Ok(true) => {
                    let relative = path
                        .strip_prefix(&root_path)
                        .expect("scanned path is inside the scan root")
                        .to_string_lossy()
                        .replace('\\', "/");
                    offenders.push(format!("{scan_root}/{relative}"));
                }
                Ok(false) => {}
                Err(error) => offenders.push(format!(
                    "{scan_root}/{} (Rust parse failed: {error})",
                    path.strip_prefix(&root_path)
                        .expect("scanned path is inside the scan root")
                        .to_string_lossy()
                        .replace('\\', "/")
                )),
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "new inline observation fold in render module(s): {offenders:?} — move the \
         read into the page's data-layer module (ARCH.md §8.1) and pass the folded \
         ViewModel down; renderer exceptions are not accepted."
    );
}

#[test]
fn observation_detection_is_per_call_not_file_wide() {
    let source = r#"
        fn content() -> impl Scene {
            let _ = sensor.current_value();
            let _ = process.current_cpu_percentage();
            bsn! { Node {} }
        }
    "#;
    assert!(source_has_inline_observation_fold(source, Frontend::Bevy).unwrap());
}

#[test]
fn non_observation_calls_do_not_hide_or_create_a_finding() {
    let source = r#"
        fn content() -> impl Scene {
            let _ = sensor.current_value();
            let _ = "alerts.current_value";
            bsn! { Node {} }
        }
    "#;
    assert!(!source_has_inline_observation_fold(source, Frontend::Bevy).unwrap());
}

#[test]
fn observation_detection_enters_scene_macro_tokens() {
    let source = r#"
        fn content() -> impl Scene {
            bsn! { Text({ process.current_cpu_percentage() }) }
        }
    "#;
    assert!(source_has_inline_observation_fold(source, Frontend::Bevy).unwrap());
}
