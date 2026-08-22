#![cfg(feature = "lang-fstar")]
//! Regression tests for issue #346: a lone candidate whose file extension was
//! not in `lang_from_path` was accepted as a call target across languages.
//!
//! Before the fix, `lang_from_path` returned one shared `"unknown"` tag for
//! every extension outside its map, and the single-candidate guard lowered
//! confidence only when *both* sides carried a known tag. A candidate in an
//! unmapped file therefore failed the guard's own precondition and kept full
//! confidence. Measured on the pre-fix resolver with these four fixtures, the
//! Rust caller scored 0.9 against `defn.rs`, 0.5 against `defn.py`, 0.5
//! against `defn.go`, and 0.9 against `defn.fst`; the accept threshold is
//! `confidence >= 0.6`, so only the F* case slipped through.
//!
//! Two checks keep a zero-edge assertion honest. Each test asserts the
//! definition was extracted at all, and `same_language_candidate_still_resolves`
//! asserts the resolver still produces an edge, so an extractor or a resolver
//! that produced nothing could not satisfy the rest.

use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;
use tokensave::tokensave::TokenSave;
use tokensave::types::{Edge, Node};

/// Builds a two-file project: a Rust caller invoking `zqmagnitude()` on an
/// `i64`, which has no such method, and one definition of that name in
/// `defn_file`. The name is deliberately unlikely to occur anywhere else.
async fn setup_project(defn_file: &str, defn_body: &str) -> (TempDir, TokenSave) {
    let dir = TempDir::new().unwrap();
    let project = dir.path();

    fs::write(
        project.join("caller.rs"),
        "pub fn measure(x: i64) -> i64 {\n    x.zqmagnitude()\n}\n",
    )
    .unwrap();
    fs::write(project.join(defn_file), defn_body).unwrap();

    let cg = TokenSave::init(project).await.unwrap();
    cg.index_all().await.unwrap();
    (dir, cg)
}

/// Number of edges of any kind whose target is a node named `name`.
fn edges_into(edges: &[Edge], by_id: &HashMap<String, &Node>, name: &str) -> usize {
    edges
        .iter()
        .filter(|e| by_id.get(&e.target).map(|n| n.name.as_str()) == Some(name))
        .count()
}

/// Indexes a fresh project and returns two counts: nodes named `zqmagnitude`,
/// and edges landing on one.
async fn incoming_edge_count(defn_file: &str, defn_body: &str) -> (usize, usize) {
    let (_dir, cg) = setup_project(defn_file, defn_body).await;
    let nodes = cg.get_all_nodes().await.unwrap();
    let edges = cg.get_all_edges().await.unwrap();
    let by_id: HashMap<String, &Node> = nodes.iter().map(|n| (n.id.clone(), n)).collect();
    let definitions = nodes.iter().filter(|n| n.name == "zqmagnitude").count();
    (definitions, edges_into(&edges, &by_id, "zqmagnitude"))
}

const FSTAR_DEFN: &str = "module Fixture\n\nval zqmagnitude : x:int -> int\n";
const PYTHON_DEFN: &str = "def zqmagnitude(x):\n    return x\n";
const GO_DEFN: &str = "package fixture\n\nfunc zqmagnitude(x int) int { return x }\n";
const RUST_DEFN: &str = "pub fn zqmagnitude(x: i64) -> i64 { x }\n";

#[tokio::test]
async fn same_language_candidate_still_resolves() {
    let (definitions, edges) = incoming_edge_count("defn.rs", RUST_DEFN).await;
    assert_eq!(
        definitions, 1,
        "the fixture must produce exactly one definition"
    );
    assert_eq!(
        edges, 1,
        "a Rust caller must keep its edge into a Rust definition; if this fails, \
         the zero-edge assertions in this file prove nothing"
    );
}

#[tokio::test]
async fn mapped_cross_language_candidate_is_refused() {
    // Python and Go are both in `lang_from_path`, so the existing guard already
    // covers them. These two cases pass before the fix and must keep passing.
    let (definitions, edges) = incoming_edge_count("defn.py", PYTHON_DEFN).await;
    assert_eq!(
        definitions, 1,
        "the Python fixture must produce a definition"
    );
    assert_eq!(edges, 0, "a Rust caller must not reach a Python definition");

    let (definitions, edges) = incoming_edge_count("defn.go", GO_DEFN).await;
    assert_eq!(definitions, 1, "the Go fixture must produce a definition");
    assert_eq!(edges, 0, "a Rust caller must not reach a Go definition");
}

#[tokio::test]
async fn unmapped_cross_language_candidate_is_refused() {
    // `.fst` is not in `lang_from_path`. This is the case issue #346 reports:
    // before the fix the caller reaches the F* definition.
    let (definitions, edges) = incoming_edge_count("defn.fst", FSTAR_DEFN).await;
    assert_eq!(definitions, 1, "the F* fixture must produce a definition");
    assert_eq!(
        edges, 0,
        "a Rust caller must not reach an F* definition just because `.fst` \
         carries no language tag"
    );
}
