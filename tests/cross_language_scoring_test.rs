#![cfg(feature = "lang-qbasic")]
//! Regression test for the second half of issue #346: when several candidates
//! share a name, the scoring pass rewarded a same-language file and penalised a
//! different-language one, but a file whose extension was not in
//! `lang_from_path` escaped both terms and won by default.
//!
//! `score_candidate` (`src/resolution/resolver.rs`) applies its language term
//! only when both sides carry a known tag. Measured on the pre-fix resolver
//! with this fixture, the language term gave `cand_mapped.rs` a delta of -80
//! and `cand_unmapped.qb` a delta of 0, and the QuickBASIC file is the one the
//! edge lands on: the only candidate written in a real language was the only
//! one penalised for it.

use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;
use tokensave::tokensave::TokenSave;
use tokensave::types::{Edge, Node};

const CALLER_PY: &str = "def run(obj):\n    return obj.zqtarget()\n";
const CAND_RS: &str = "pub fn zqtarget() -> i64 { 1 }\n";
const CAND_QB: &str = "FUNCTION zqtarget ()\n    zqtarget = 1\nEND FUNCTION\n";
const CAND_PY: &str = "def zqtarget():\n    return 1\n";

/// Indexes a project made of `files` and returns the number of nodes named
/// `zqtarget`, the number of edges landing on one, and the file the first such
/// edge points at.
async fn resolve(files: &[(&str, &str)]) -> (usize, usize, Option<String>) {
    let dir = TempDir::new().unwrap();
    let project = dir.path();
    fs::write(project.join("caller.py"), CALLER_PY).unwrap();
    for (name, body) in files {
        fs::write(project.join(name), body).unwrap();
    }

    let cg = TokenSave::init(project).await.unwrap();
    cg.index_all().await.unwrap();

    let nodes = cg.get_all_nodes().await.unwrap();
    let edges = cg.get_all_edges().await.unwrap();
    let by_id: HashMap<String, &Node> = nodes.iter().map(|n| (n.id.clone(), n)).collect();

    let candidates = nodes.iter().filter(|n| n.name == "zqtarget").count();
    let landed: Vec<&Edge> = edges
        .iter()
        .filter(|e| by_id.get(&e.target).map(|n| n.name.as_str()) == Some("zqtarget"))
        .collect();
    let winner = landed
        .first()
        .and_then(|e| by_id.get(&e.target))
        .map(|n| n.file_path.clone());
    (candidates, landed.len(), winner)
}

#[tokio::test]
async fn same_language_candidate_resolves() {
    // Control for resolution itself. With one candidate this never reaches
    // `score_candidate`; it only proves the caller can reach a definition at
    // all, so a resolver that resolved nothing could not pass the rest.
    let (candidates, edges, winner) = resolve(&[("cand_same.py", CAND_PY)]).await;
    assert_eq!(candidates, 1, "the fixture must produce one candidate");
    assert_eq!(edges, 1, "a Python caller must reach a Python definition");
    assert_eq!(winner.as_deref(), Some("cand_same.py"));
}

#[tokio::test]
async fn same_language_candidate_wins_over_a_mapped_rival() {
    // Control for the scoring path, which is what this file is about. Two
    // candidates, one sharing the caller's language, so the language term is
    // the thing that decides. This must hold before and after the fix.
    let (candidates, edges, winner) =
        resolve(&[("cand_same.py", CAND_PY), ("cand_mapped.rs", CAND_RS)]).await;
    assert_eq!(candidates, 2, "both candidates must be extracted");
    assert_eq!(edges, 1, "the same-language candidate must win outright");
    assert_eq!(
        winner.as_deref(),
        Some("cand_same.py"),
        "scoring must prefer the candidate in the caller's own language"
    );
}

#[tokio::test]
async fn unmapped_candidate_does_not_win_by_default() {
    let (candidates, edges, winner) =
        resolve(&[("cand_mapped.rs", CAND_RS), ("cand_unmapped.qb", CAND_QB)]).await;
    assert_eq!(
        candidates, 2,
        "both candidates must be extracted, otherwise there is nothing to choose between"
    );
    assert_eq!(
        edges, 0,
        "neither candidate shares the caller's language, so neither should win; \
         got {winner:?}"
    );
}
