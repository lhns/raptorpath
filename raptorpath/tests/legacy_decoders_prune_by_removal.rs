//! Routing gate for the legacy decoders' per-drain prune (paper §5.2,
//! status §32): `advance` runs once per receiver drain, so its cost must be
//! O(entries it removes) like the unified decoder's — ordered sets popped
//! from the front. A source scan, because the property is the shape of the
//! statement: a `retain`, an `iter_mut` over all rows, a `keys()` walk or a
//! `collect` is O(everything retained) per drain. Behaviour (the contract the
//! prune must keep) is pinned in `src/fec/prune_contract_tests.rs`.

const RLC: &str = include_str!("../src/fec/rlc_window.rs");
const GEN: &str = include_str!("../src/fec/generation.rs");
const REF: &str = include_str!("../src/fec/generation/reference.rs");

/// The body of the `fn advance` inside `impl WindowDecoder for <ty>`, with
/// `//` comments removed (the gate reads statements, not prose).
fn advance_body(src: &str, ty: &str) -> String {
    let imp = src
        .find(&format!("impl WindowDecoder for {ty}"))
        .unwrap_or_else(|| panic!("no `impl WindowDecoder for {ty}`"));
    let from = imp + src[imp..].find("fn advance(").expect("decoder advance");
    let open = from + src[from..].find('{').expect("advance body");
    let code: String = src[open..]
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let mut depth = 0usize;
    for (i, c) in code.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return code[..=i].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced advance body of {ty}");
}

#[test]
fn legacy_decoder_advance_touches_only_what_it_removes() {
    for (ty, src) in [
        ("RlcWindowDecoder", RLC),
        ("GenerationDecoder", GEN),
        ("RefGenerationDecoder", REF),
    ] {
        let body = advance_body(src, ty);
        assert!(body.len() > 100, "{ty}: the extracted body looks wrong:\n{body}");
        for banned in [".retain(", ".iter_mut()", ".keys()", ".collect"] {
            assert!(
                !body.contains(banned),
                "{ty}::advance contains `{banned}` — a per-drain scan:\n{body}"
            );
        }
        assert!(
            body.contains("pop_first") || body.contains("first_entry"),
            "{ty}::advance must prune by popping from the front:\n{body}"
        );
    }
}
