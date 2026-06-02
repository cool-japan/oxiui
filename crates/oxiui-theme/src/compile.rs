//! Compiled stylesheet for ~O(1) widget→style lookup.
//!
//! [`CompiledStyleSheet`] pre-buckets rules from a [`StyleSheet`] by their
//! primary selector part (type name, class name, or id) so that resolving
//! a widget's style only needs to examine the relevant subset of rules rather
//! than the full rule list.
//!
//! The cascade result is **identical** to [`StyleSheet::compute_style`]:
//! specificity ordering, first-matching-selector semantics, and source-order
//! tie-breaking are all preserved.

use std::collections::HashMap;

use crate::stylesheet::{
    apply_rule, selector_matches, ComputedStyle, Rule, SelectorPart, Specificity, StyleSheet,
};

// ── CompiledStyleSheet ─────────────────────────────────────────────────────────

/// A compiled stylesheet: rules indexed into buckets keyed by their primary
/// selector part so that widget→style resolution is ~O(1) for common cases.
///
/// Rules are stored by-index in the `rules` vec; each bucket holds a sorted
/// list of rule indices.  Dedup is applied during lookup so a rule whose
/// selector list covers multiple buckets is only counted once.
///
/// Construct via [`CompiledStyleSheet::compile`].
pub struct CompiledStyleSheet {
    /// All rules from the original stylesheet (clone, so no lifetime param).
    rules: Vec<Rule>,
    /// Indices of rules whose first `SelectorPart` is a type name.
    type_rules: HashMap<String, Vec<usize>>,
    /// Indices of rules whose first `SelectorPart` is a class name.
    class_rules: HashMap<String, Vec<usize>>,
    /// Indices of rules whose first `SelectorPart` is an id.
    id_rules: HashMap<String, Vec<usize>>,
    /// Indices of rules with no specific primary key (fallback).
    universal_rules: Vec<usize>,
    /// Generation counter — incremented when compiled from a new stylesheet.
    pub generation: u64,
}

impl CompiledStyleSheet {
    /// Compile a parsed [`StyleSheet`] into a [`CompiledStyleSheet`].
    ///
    /// `generation` is stored on the struct and used by [`StyleCache`][crate::StyleCache]
    /// to detect when a compiled stylesheet has changed.
    pub fn compile(sheet: &StyleSheet, generation: u64) -> Self {
        let mut type_rules: HashMap<String, Vec<usize>> = HashMap::new();
        let mut class_rules: HashMap<String, Vec<usize>> = HashMap::new();
        let mut id_rules: HashMap<String, Vec<usize>> = HashMap::new();
        let mut universal_rules: Vec<usize> = Vec::new();

        for (idx, rule) in sheet.rules.iter().enumerate() {
            // A rule may have multiple selectors.  We bucket by the first part
            // of *each* selector so every candidate bucket gets the index, and
            // dedup during lookup prevents double-application.
            if rule.selectors.is_empty() {
                universal_rules.push(idx);
                continue;
            }

            let mut bucketed = false;
            for selector in &rule.selectors {
                if let Some(first_part) = selector.parts.first() {
                    bucketed = true;
                    match first_part {
                        SelectorPart::Type(name) => {
                            type_rules.entry(name.clone()).or_default().push(idx);
                        }
                        SelectorPart::Class(name) => {
                            class_rules.entry(name.clone()).or_default().push(idx);
                        }
                        SelectorPart::Id(name) => {
                            id_rules.entry(name.clone()).or_default().push(idx);
                        }
                    }
                } else {
                    // Selector with no parts — treat as universal.
                    universal_rules.push(idx);
                }
            }

            if !bucketed {
                universal_rules.push(idx);
            }
        }

        // Sort each bucket by source_order ascending (stable iteration order).
        let sort_by_source = |indices: &mut Vec<usize>, rules: &[Rule]| {
            indices.sort_by_key(|&i| rules[i].source_order);
            indices.dedup();
        };

        for v in type_rules.values_mut() {
            sort_by_source(v, &sheet.rules);
        }
        for v in class_rules.values_mut() {
            sort_by_source(v, &sheet.rules);
        }
        for v in id_rules.values_mut() {
            sort_by_source(v, &sheet.rules);
        }
        sort_by_source(&mut universal_rules, &sheet.rules);

        Self {
            rules: sheet.rules.clone(),
            type_rules,
            class_rules,
            id_rules,
            universal_rules,
            generation,
        }
    }

    /// Compute the final [`ComputedStyle`] for a widget.
    ///
    /// The result is semantically identical to [`StyleSheet::compute_style`]:
    ///
    /// 1. Candidate rules are collected from the relevant buckets (type, each
    ///    class, id, and universal).
    /// 2. Rule indices are deduplicated.
    /// 3. For each unique rule, the first matching selector's specificity is used
    ///    (mirroring the `break` in `StyleSheet::matching_rules`).
    /// 4. Rules are sorted by `(specificity, source_order)` ascending and applied
    ///    in that order (last-writer-wins per property).
    pub fn compute_style(
        &self,
        widget_type: &str,
        classes: &[&str],
        id: Option<&str>,
    ) -> ComputedStyle {
        // 1. Collect candidate rule indices from buckets.
        let mut candidate_indices: Vec<usize> = Vec::new();

        if let Some(idxs) = self.type_rules.get(widget_type) {
            candidate_indices.extend_from_slice(idxs);
        }
        for class in classes {
            if let Some(idxs) = self.class_rules.get(*class) {
                candidate_indices.extend_from_slice(idxs);
            }
        }
        if let Some(id_str) = id {
            if let Some(idxs) = self.id_rules.get(id_str) {
                candidate_indices.extend_from_slice(idxs);
            }
        }
        candidate_indices.extend_from_slice(&self.universal_rules);

        // 2. Deduplicate while preserving order.
        candidate_indices.sort_unstable();
        candidate_indices.dedup();

        // 3. For each candidate, run the same first-matching-selector logic as
        //    the original `matching_rules` — if no selector matches, skip the rule.
        let mut matches: Vec<(usize, Specificity)> = Vec::new();
        for idx in candidate_indices {
            let rule = &self.rules[idx];
            for selector in &rule.selectors {
                if selector_matches(selector, widget_type, classes, id) {
                    matches.push((idx, selector.specificity));
                    break; // first matching selector for this rule — same as original
                }
            }
        }

        // 4. Sort by (specificity, source_order) ascending; apply in order.
        matches.sort_by(|a, b| {
            a.1.cmp(&b.1).then(
                self.rules[a.0]
                    .source_order
                    .cmp(&self.rules[b.0].source_order),
            )
        });

        let mut result = ComputedStyle::default();
        for (idx, _) in &matches {
            apply_rule(&mut result, &self.rules[*idx].style);
        }
        result
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stylesheet::StyleSheet;

    fn compile_css(css: &str) -> (StyleSheet, CompiledStyleSheet) {
        let sheet = StyleSheet::parse(css).stylesheet;
        let compiled = CompiledStyleSheet::compile(&sheet, 1);
        (sheet, compiled)
    }

    /// Helper: assert compiled and uncompiled produce the same style for all
    /// test inputs.
    fn check_equivalence(css: &str, inputs: &[(&str, Vec<&str>, Option<&str>)]) {
        let (sheet, compiled) = compile_css(css);
        for (wtype, classes, id) in inputs {
            let expected = sheet.compute_style(wtype, classes, *id);
            let actual = compiled.compute_style(wtype, classes, *id);
            assert_eq!(
                expected, actual,
                "divergence for widget_type={wtype:?} classes={classes:?} id={id:?}"
            );
        }
    }

    // ── Equivalence tests ─────────────────────────────────────────────────────

    #[test]
    fn test_compiled_matches_uncompiled_simple_selector() {
        check_equivalence(
            ".button { color: #ff0000; }",
            &[
                ("button", vec!["button"], None),
                ("label", vec!["button"], None),
                ("button", vec![], None),
            ],
        );
    }

    #[test]
    fn test_compiled_matches_uncompiled_compound_selector() {
        check_equivalence(
            ".button.primary { background: #0000ff; }",
            &[
                ("button", vec!["button", "primary"], None),
                ("button", vec!["button"], None),
                ("button", vec!["primary"], None),
                ("label", vec!["button", "primary"], None),
            ],
        );
    }

    #[test]
    fn test_compiled_matches_uncompiled_grouped_selector() {
        check_equivalence(
            "button, label { color: #000000; }",
            &[
                ("button", vec![], None),
                ("label", vec![], None),
                ("input", vec![], None),
            ],
        );
    }

    #[test]
    fn test_specificity_tiebreak_preserved_post_compile() {
        // More specific rule (#id) must win over type rule.
        check_equivalence(
            "button { color: #ff0000; } #submit { color: #00ff00; }",
            &[("button", vec![], Some("submit")), ("button", vec![], None)],
        );
    }

    /// Regression: grouped selector where widget matches both selectors of a
    /// single rule must not apply the rule twice and must not produce wrong
    /// specificity compared to the uncompiled path.
    #[test]
    fn test_compiled_matches_uncompiled_ambiguous_grouped() {
        // Rule 0: `button, .foo { color: #ff0000 }` — specificity via `.foo`
        //          is (0,1,0), via `button` is (0,0,1).
        //          For widget type=button with class=foo: original picks
        //          `button` selector first → spec=(0,0,1); colour = red.
        // Rule 1: `button { color: #0000ff }` — spec=(0,0,1), source_order=1.
        //          Same specificity, higher source_order → blue wins.
        // So final colour must be blue (#0000ff), not red.
        check_equivalence(
            "button, .foo { color: #ff0000; } button { color: #0000ff; }",
            &[
                ("button", vec!["foo"], None),
                ("button", vec![], None),
                ("label", vec!["foo"], None),
            ],
        );
    }

    /// Broad cross-check loop over multiple inputs for a realistic stylesheet.
    #[test]
    fn test_compiled_matches_uncompiled_cross_check() {
        let css = r#"
            button { color: #111111; padding: 8px; }
            .primary { background: #7aa2f7; }
            button.primary { font-size: 14px; }
            #cancel { color: #ff0000; }
            label, input { font-size: 12px; }
            .disabled { opacity: 0.5; }
        "#;
        let inputs: &[(&str, Vec<&str>, Option<&str>)] = &[
            ("button", vec![], None),
            ("button", vec!["primary"], None),
            ("button", vec!["primary", "disabled"], None),
            ("button", vec!["disabled"], Some("cancel")),
            ("label", vec![], None),
            ("input", vec!["primary"], None),
            ("input", vec!["disabled"], None),
            ("span", vec![], None),
        ];
        check_equivalence(css, inputs);
    }
}
