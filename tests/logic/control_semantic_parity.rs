//! source-inspection: static-policy
//!
//! Control-semantic parity gate (ARCH.md §8.1 语义平价律, 2026-08-19).
//!
//! [`super::control_vocabulary_boundary`] pins the NEGATIVE side of the
//! control vocabulary (no raw nice payloads, no POSIX stop/continue signal
//! names in a UI tree). This file pins the POSITIVE side: the frontends must
//! OFFER the same control semantics — the same priority tier
//! set, the same suspend/resume vocabulary, and the one shared tier→label
//! fold. `dual_track_policy_parity` is the behavioral template for policy
//! semantics; this gate extends the same discipline to the control surface,
//! the way `renderer_fold_boundary` extends the display fold.
//!
//! The scan roots cover all four product frontends. The designated OFFER
//! surfaces (`TIER_OFFER_FILES`) name one file per frontend: Bevy's is the
//! Applications action menu (`pages/processes/menu.rs`), which offers the
//! same three tiers through the shell's batch track as the other three.
//!
//! Scanning is deliberately structural, mirroring
//! [`super::control_vocabulary_boundary`]: a parity gate on the vocabulary
//! SURFACE is a boundary check (the repo's test red line allows structural
//! guards for firewall/boundary contracts), and the behavioral coverage of
//! each pinned mapping already lives beside the code it pins:
//! * core `core/process/control.rs` unit tests cover `PriorityTier::ALL`
//!   and `i18n_key`;
//! * iced `ui/applications/priority_choice.rs` unit tests cover the
//!   preset→tier mapping and the localized labels;
//! * GPUI `root/proc_action.rs` has a pure mapping test plus a
//!   `gpui::test` (`menu_suspend_resume_submit_the_neutral_request`) that
//!   drives the real menu through a recording platform client.

use std::fs;
use std::path::{Path, PathBuf};

const SCAN_ROOTS: [&str; 4] = [
    "crates/taskmanager-gpui/src",
    "crates/taskmanager-tui/src",
    "crates/taskmanager-iced/src",
    "crates/taskmanager-bevy-ui/src",
];

/// The priority-preset surface of each frontend — the one file whose typed
/// tier references constitute the offer (GPUI action-bar presets, TUI
/// process-menu picker, Iced pick_list model, Bevy Applications action menu).
const TIER_OFFER_FILES: [(&str, &str); 4] = [
    (
        "GPUI",
        "crates/taskmanager-gpui/src/gpui_app/processes_view/chrome/action_bar.rs",
    ),
    ("TUI", "crates/taskmanager-tui/src/ui/process_menu.rs"),
    (
        "Iced",
        "crates/taskmanager-iced/src/ui/applications/priority_choice.rs",
    ),
    (
        "Bevy",
        "crates/taskmanager-bevy-ui/src/pages/processes/menu.rs",
    ),
];

/// The neutral tier vocabulary every frontend must offer, in canonical
/// order (`PriorityTier::ALL` order).
const TIERS: [&str; 3] = ["High", "Normal", "Low"];

const GPUI_MENU_SUBMISSION: &str = "crates/taskmanager-gpui/src/gpui_app/root/proc_action.rs";
const TUI_MENU_DISPATCH: &str = "crates/taskmanager-tui/src/menus.rs";
const ICED_MENU_DISPATCH: &str = "crates/taskmanager-iced/src/app/process_menu.rs";
const SHELL_PRESENTATION: &str = "crates/taskmanager-shell/src/presentation.rs";

/// How far past a match-arm head the gate searches for the mapped request.
const ARM_LOOKAHEAD: usize = 300;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Read one designated source file with line comments stripped. Panics on a
/// missing file: a moved/renamed parity surface must fail loudly, not skip.
fn read_stripped(relative: &str) -> String {
    let full = repository().join(relative);
    let source = fs::read_to_string(&full)
        .unwrap_or_else(|error| panic!("parity-gate source file {relative}: {error}"));
    strip_line_comments(&source)
}

/// The distinct typed tiers a preset surface references (`PriorityTier::T`
/// occurrences, deduplicated, canonical order).
fn tier_offer(code: &str) -> Vec<&'static str> {
    TIERS
        .iter()
        .copied()
        .filter(|tier| code.contains(&format!("PriorityTier::{tier}")))
        .collect()
}

/// Tier parity (§8.1 语义平价律): each designated priority surface offers
/// EXACTLY the neutral `PriorityTier` set {High, Normal, Low} — a frontend
/// that drops a tier or invents one makes the same control command mean
/// different things per frontend, which is a parity bug, not a style choice.
/// The platform adapter owns tier→native; nothing here may widen to raw
/// numbers (that negative side is `control_vocabulary_boundary`). Bevy is
/// not designated yet — a surface must exist before it can be pinned.
#[test]
fn the_designated_surfaces_offer_exactly_the_high_normal_low_priority_tiers() {
    for (frontend, rel_path) in TIER_OFFER_FILES {
        let code = read_stripped(rel_path);
        let offered = tier_offer(&code);
        assert_eq!(
            offered, TIERS,
            "{frontend} priority offer drifted: found {offered:?} in {rel_path}, expected \
             exactly [High, Normal, Low] (ARCH.md §8.1 语义平价律: every frontend must \
             offer the same tier set; a missing tier is a dropped capability, an extra tier \
             is a private vocabulary)."
        );

        match frontend {
            // GPUI's presets submit the typed batch action from two lanes
            // (multi-select batch + single-target immediate), so each tier
            // token legitimately appears twice; the pinned invariant is the
            // OFFER SET, not the occurrence count.
            "GPUI" => assert!(
                code.contains("ProcessBatchAction::SetPriority"),
                "{frontend} presets stopped constructing the typed \
                 ProcessBatchAction::SetPriority in {rel_path}"
            ),
            // Bevy's action menu submits the same typed batch action through
            // the shell's single-row batch track, and labels its tiers only
            // through the shared fold.
            "Bevy" => {
                assert!(
                    code.contains("ProcessBatchAction::SetPriority"),
                    "{frontend} action menu stopped constructing the typed \
                     ProcessBatchAction::SetPriority in {rel_path}"
                );
                assert!(
                    code.contains("presentation::priority_tier_label"),
                    "{frontend} action menu stopped labelling tiers through the shared \
                     priority_tier_label fold in {rel_path}"
                );
            }
            // TUI's picker maps three ProcessMenuAction variants through the
            // `priority_tier` fold in ui/process_menu.rs.
            "TUI" => {
                assert!(
                    code.contains("fn priority_tier("),
                    "{frontend} lost the priority_tier mapping in {rel_path}"
                );
                for tier in TIERS {
                    assert!(
                        code.contains(&format!("ProcessMenuAction::Priority{tier}")),
                        "{frontend} lost the Priority{tier} menu variant in {rel_path}"
                    );
                }
            }
            // Iced's pick_list renders PriorityChoice::ALL, so the enum's
            // variant count is the offer count.
            "Iced" => {
                assert!(
                    code.contains("enum PriorityChoice"),
                    "{frontend} lost the PriorityChoice pick_list model in {rel_path}"
                );
                assert!(
                    code.contains("const ALL: [Self; 3]"),
                    "{frontend} PriorityChoice::ALL is no longer exactly three variants \
                     in {rel_path}"
                );
                for tier in TIERS {
                    assert!(
                        code.contains(&format!("Self::{tier} =>")),
                        "{frontend} PriorityChoice lost its {tier} arm in {rel_path}"
                    );
                }
            }
            _ => unreachable!("TIER_OFFER_FILES names only the four frontends"),
        }
    }
}

/// Suspend/resume parity (§8.1 语义完备律 + 语义平价律): every frontend
/// expresses the suspend CONCEPT through the neutral vocabulary — GPUI's
/// direct track composes `ProcessControlRequest::Suspend/Resume`, TUI/Iced/
/// Bevy submit `ProcessBatchAction::Suspend/Resume` through the shell batch
/// track. The two lanes are the documented dual-track split (ADR-027) and
/// are both legitimate; this gate pins the VOCABULARY (the concept is never
/// expressed as a stop/continue signal — that negative side is
/// `control_vocabulary_boundary`), not the lane. Behavioral proof for GPUI
/// lives beside the mapping (`menu_suspend_resume_submit_the_neutral_request`
/// drives the real menu through a recording platform client), so the GPUI
/// side here stays structural-light on purpose.
#[test]
fn suspend_resume_reach_every_adapter_through_the_neutral_vocabulary() {
    // GPUI direct track: every `MenuControlRequest::<Concept> =>` arm of
    // `menu_control_submission` must compose `ProcessControlRequest::<Concept>`.
    let gpui = read_stripped(GPUI_MENU_SUBMISSION);
    assert!(
        gpui.contains("fn menu_control_submission"),
        "GPUI lost menu_control_submission in {GPUI_MENU_SUBMISSION}"
    );
    for concept in ["Suspend", "Resume"] {
        let arm = format!("MenuControlRequest::{concept} =>");
        let mut arms = 0usize;
        let mut head = 0usize;
        while let Some(found) = gpui[head..].find(&arm) {
            let start = head + found;
            let arm_end = (start + ARM_LOOKAHEAD).min(gpui.len());
            assert!(
                gpui[start..arm_end].contains(&format!("ProcessControlRequest::{concept}")),
                "GPUI menu_control_submission maps {concept} to something other than \
                 ProcessControlRequest::{concept} in {GPUI_MENU_SUBMISSION} — the neutral \
                 request is the only legal spelling of the concept (§8.1 语义完备律)."
            );
            arms += 1;
            head = start + arm.len();
        }
        assert!(
            arms > 0,
            "GPUI menu_control_submission no longer carries a {concept} arm in \
             {GPUI_MENU_SUBMISSION}"
        );
    }

    // TUI + Iced + Bevy shell batch track: the process menu must submit the
    // neutral batch action for both concepts.
    for (frontend, rel_path) in [
        ("TUI", TUI_MENU_DISPATCH),
        ("Iced", ICED_MENU_DISPATCH),
        (
            "Bevy",
            "crates/taskmanager-bevy-ui/src/pages/processes/menu.rs",
        ),
    ] {
        let code = read_stripped(rel_path);
        for concept in ["Suspend", "Resume"] {
            assert!(
                code.contains(&format!(
                    "request_process_batch(ProcessBatchAction::{concept})"
                )),
                "{frontend} process menu stopped submitting \
                 ProcessBatchAction::{concept} in {rel_path} — the neutral batch action is \
                 the only legal spelling of the concept on the shell track (§8.1)."
            );
        }
    }
}

/// Label-fold parity (§8.1 同一律): exactly ONE tier→label fold exists —
/// `taskmanager_shell::presentation::priority_tier_label`, routing through
/// `tier.i18n_key()`. Every frontend that labels tiers must read that fold
/// (directly or via `tier.i18n_key()`), and any local
/// `fn priority_tier_label` in a UI tree must be a pure delegation wrapper,
/// never a re-implementation (two folds drift apart the first time one
/// catalog entry is edited).
#[test]
fn the_priority_tier_label_fold_is_single_sourced_and_read_by_every_frontend() {
    // The single fold exists and routes through the tier's own catalog key.
    let shell = read_stripped(SHELL_PRESENTATION);
    let Some(fold_at) = shell.find("pub fn priority_tier_label") else {
        panic!(
            "the single priority tier label fold disappeared from {SHELL_PRESENTATION} — \
             it must live in the shell presentation layer (§8.1 同一律)"
        );
    };
    let fold_end = (fold_at + 400).min(shell.len());
    assert!(
        shell[fold_at..fold_end].contains("i18n_key()"),
        "the shell priority_tier_label fold no longer routes through tier.i18n_key() — \
         a hand-rolled catalog table inside the fold is a drift seed"
    );

    // Every frontend references the shared fold, and any local
    // `fn priority_tier_label` definition is a delegation to it.
    let repo = repository();
    for scan_root in SCAN_ROOTS {
        let root = repo.join(scan_root);
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(!files.is_empty(), "scan root missing: {scan_root}");

        let mut referencing_files = 0usize;
        let mut offenders = Vec::new();
        for path in &files {
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            let code = strip_line_comments(&source);
            let references_fold = code.contains("presentation::priority_tier_label")
                || (code.contains("i18n_key()") && code.contains("PriorityTier"));
            if references_fold {
                referencing_files += 1;
            }

            // A local definition is legal only as a thin delegation wrapper.
            let lines: Vec<&str> = code.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if !line.contains("fn priority_tier_label") {
                    continue;
                }
                let wrapper_end = (index + 7).min(lines.len());
                let body = lines[index..wrapper_end].join("\n");
                if !body.contains("presentation::priority_tier_label") {
                    let relative = path
                        .strip_prefix(&repo)
                        .expect("scanned path is inside the repository")
                        .to_string_lossy()
                        .replace('\\', "/");
                    offenders.push(format!(
                        "{relative}: fn priority_tier_label does not delegate to the \
                         shell fold (a private tier→label table drifts from the single \
                         source the first time a catalog entry is edited)"
                    ));
                }
            }
        }

        assert!(
            referencing_files > 0,
            "{scan_root} never references the shared tier label fold \
             (presentation::priority_tier_label or tier.i18n_key()) — a frontend that \
             labels tiers must read the single fold (§8.1 同一律)"
        );
        assert!(
            offenders.is_empty(),
            "private priority tier label folds in {scan_root}: {offenders:?} — the one \
             fold lives in taskmanager_shell::presentation::priority_tier_label"
        );
    }
}
