//! Line-patch primitives shared by the editor and the host store.
//!
//! Every diff is emitted as a list of *disjoint* contiguous blocks, one per
//! region that actually changed, with untouched lines separating them. This is
//! what makes concurrent edits *rebased* safely: each removed block (`old`) is
//! located independently inside newer content and replaced with the new lines,
//! so a concurrent edit that only touches one region never forces the other
//! regions into a conflict. Overlapping edits (where a removed block is no
//! longer present) fall back to last-write-wins.

use crate::sync::protocol::LinePatch;

/// Guard for the LCS used to split a diff into blocks: above this many cells we
/// fall back to treating the whole changed middle as a single block rather than
/// paying a quadratic cost (only happens for pathological far-apart edits).
const MAX_LCS_CELLS: usize = 200_000;

/// The disjoint block-diff turning `old` into `new`.
pub fn compute_line_patches(old: &[String], new: &[String]) -> Vec<LinePatch> {
    let mut start = 0;
    while start < old.len() && start < new.len() && old[start] == new[start] {
        start += 1;
    }
    let mut old_end = old.len();
    let mut new_end = new.len();
    while old_end > start && new_end > start && old[old_end - 1] == new[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    if old_end == start && new_end == start {
        return vec![];
    }
    let old_mid = &old[start..old_end];
    let new_mid = &new[start..new_end];
    let mut shift: isize = 0;
    split_hunks(old_mid, new_mid)
        .into_iter()
        .map(|(o0, n0, ol, nl)| {
            // `start` is relative to the base array the blocks are applied to
            // *in order*: offset by the net length change of all previous
            // blocks so a direct sequential apply lands on the right lines.
            let at = (start as isize + o0 as isize + shift).max(0) as usize;
            shift += nl as isize - ol as isize;
            LinePatch {
                start: at,
                remove: ol,
                old: old[start + o0..start + o0 + ol].to_vec(),
                prev: if start + o0 > 0 {
                    Some(old[start + o0 - 1].clone())
                } else {
                    None
                },
                next: if start + o0 + ol < old.len() {
                    Some(old[start + o0 + ol].clone())
                } else {
                    None
                },
                lines: new[start + n0..start + n0 + nl].to_vec(),
            }
        })
        .collect()
}

/// Split the changed middle `old[..]` vs `new[..]` into disjoint hunks, each
/// `(old_start, new_start, old_len, new_len)`, so runs of unchanged lines
/// between separate edits become separate rebasable blocks.
fn split_hunks(om: &[String], nm: &[String]) -> Vec<(usize, usize, usize, usize)> {
    let n = om.len();
    let m = nm.len();
    if n == 0 {
        return vec![(0, 0, 0, m)];
    }
    if m == 0 {
        return vec![(0, 0, n, 0)];
    }
    if n.saturating_mul(m) > MAX_LCS_CELLS {
        return vec![(0, 0, n, m)];
    }
    // LCS lengths for om[i..] × nm[j..].
    let w = m + 1;
    let mut dp = vec![0usize; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if om[i] == nm[j] {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let mut hunks = Vec::new();
    let mut i = 0;
    let mut j = 0;
    let (mut so, mut sn) = (0, 0);
    let mut in_hunk = false;
    while i < n && j < m {
        if om[i] == nm[j] {
            if in_hunk {
                hunks.push((so, sn, i - so, j - sn));
                in_hunk = false;
            }
            i += 1;
            j += 1;
        } else {
            if !in_hunk {
                so = i;
                sn = j;
                in_hunk = true;
            }
            let del = dp[(i + 1) * w + j];
            let ins = dp[i * w + j + 1];
            if del >= ins {
                if i + 1 < n {
                    i += 1;
                } else if j + 1 < m {
                    j += 1;
                } else {
                    i += 1;
                }
            } else if j + 1 < m {
                j += 1;
            } else if i + 1 < n {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    if in_hunk || i < n || j < m {
        if !in_hunk {
            so = i;
            sn = j;
        }
        hunks.push((so, sn, n - so, m - sn));
    }
    hunks
}

/// Apply a patch to `lines` in place, blocks in order.
pub fn apply_blocks(lines: &mut Vec<String>, patches: &[LinePatch]) {
    for p in patches {
        let start = p.start.min(lines.len());
        let end = (start + p.remove).min(lines.len());
        let mut new_lines = Vec::with_capacity(lines.len() - (end - start) + p.lines.len());
        new_lines.extend_from_slice(&lines[..start]);
        new_lines.extend_from_slice(&p.lines);
        new_lines.extend_from_slice(&lines[end..]);
        *lines = new_lines;
    }
}

/// Locate `block` as a contiguous run inside `content`.
pub fn find_block(content: &[String], block: &[String]) -> Option<usize> {
    if block.is_empty() || block.len() > content.len() {
        return None;
    }
    content.windows(block.len()).position(|w| w == block)
}

/// For a pure insertion (`old` empty), find the re-anchored position in `cur`
/// using the surrounding `prev`/`next` lines captured from the diff base.
fn find_insert_pos(cur: &[String], p: &LinePatch) -> Option<usize> {
    if let (Some(prev), Some(next)) = (&p.prev, &p.next) {
        for i in 0..cur.len().saturating_sub(1) {
            if cur[i] == *prev && cur[i + 1] == *next {
                return Some(i + 1);
            }
        }
    }
    if let Some(prev) = &p.prev {
        if let Some(i) = cur.iter().rposition(|l| l == prev) {
            return Some(i + 1);
        }
    }
    if let Some(next) = &p.next {
        if let Some(i) = cur.iter().position(|l| l == next) {
            return Some(i);
        }
    }
    if p.start == 0 {
        return Some(0);
    }
    None
}

/// Rebase a patch (computed against an older base) onto the current `base`
/// content. Returns the transformed blocks, or `None` if a removed block is no
/// longer present and no anchor can be re-located (overlapping concurrent edit
/// — caller does LWW).
pub fn rebase_blocks(base: &[String], patches: &[LinePatch]) -> Option<Vec<LinePatch>> {
    let mut cur = base.to_vec();
    let mut out = Vec::with_capacity(patches.len());
    for p in patches {
        let at = if p.old.is_empty() {
            find_insert_pos(&cur, p)?
        } else {
            find_block(&cur, &p.old)?
        };
        let np = LinePatch {
            start: at,
            remove: p.old.len(),
            old: p.old.clone(),
            prev: p.prev.clone(),
            next: p.next.clone(),
            lines: p.lines.clone(),
        };
        apply_blocks(&mut cur, std::slice::from_ref(&np));
        out.push(np);
    }
    Some(out)
}

/// Rebase a diff onto newer content, returning the resulting content (or `None`
/// if a removed block was overwritten by a concurrent edit). Used to fold a
/// user's pending local edits onto newly-arrived canonical patches without
/// touching their typing buffer.
pub fn rebase_onto(content: &[String], patches: &[LinePatch]) -> Option<Vec<String>> {
    let transformed = rebase_blocks(content, patches)?;
    let mut out = content.to_vec();
    apply_blocks(&mut out, &transformed);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn diff_single_block() {
        let old = s(&["a", "b", "c", "d"]);
        let new = s(&["a", "X", "Y", "d"]);
        let p = compute_line_patches(&old, &new);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].start, 1);
        assert_eq!(p[0].remove, 2);
        assert_eq!(p[0].old, s(&["b", "c"]));
        assert_eq!(p[0].lines, s(&["X", "Y"]));
    }

    #[test]
    fn diff_empty() {
        let old = s(&["a", "b"]);
        assert!(compute_line_patches(&old, &old).is_empty());
    }

    #[test]
    fn apply_is_inverse() {
        let old = s(&["a", "b", "c", "d"]);
        let new = s(&["a", "X", "Y", "d"]);
        let p = compute_line_patches(&old, &new);
        let mut lines = old.clone();
        apply_blocks(&mut lines, &p);
        assert_eq!(lines, new);
    }

    #[test]
    fn rebase_shifts_offsets() {
        // Base content; two concurrent edits:
        //   A: replace "b" with "B"          (lines 1)
        //   B: replace "d" with "D"          (line 3)
        let a = LinePatch {
            start: 1,
            remove: 1,
            old: s(&["b"]),
            prev: Some("a".into()),
            next: Some("c".into()),
            lines: s(&["B"]),
        };
        let b = LinePatch {
            start: 3,
            remove: 1,
            old: s(&["d"]),
            prev: Some("c".into()),
            next: Some("e".into()),
            lines: s(&["D"]),
        };
        let base = s(&["a", "b", "c", "d", "e"]);

        // Apply A first.
        let mut after_a = base.clone();
        apply_blocks(&mut after_a, std::slice::from_ref(&a));
        assert_eq!(after_a, s(&["a", "B", "c", "d", "e"]));

        // Rebase B onto after_a: "d" still found (offset unchanged).
        let rebased = rebase_blocks(&after_a, std::slice::from_ref(&b)).unwrap();
        assert_eq!(rebased[0].start, 3);
        let mut merged = after_a.clone();
        apply_blocks(&mut merged, &rebased);
        assert_eq!(merged, s(&["a", "B", "c", "D", "e"]));

        // Now A inserts lines above B, shifting B's block.
        let a_insert = LinePatch {
            start: 1,
            remove: 0,
            old: vec![],
            prev: Some("a".into()),
            next: Some("b".into()),
            lines: s(&["b1", "b2"]),
        };
        let mut after_insert = base.clone();
        apply_blocks(&mut after_insert, std::slice::from_ref(&a_insert));
        assert_eq!(after_insert, s(&["a", "b1", "b2", "b", "c", "d", "e"]));
        let rebased2 = rebase_blocks(&after_insert, std::slice::from_ref(&b)).unwrap();
        assert_eq!(rebased2[0].start, 5);
        let mut merged2 = after_insert.clone();
        apply_blocks(&mut merged2, &rebased2);
        assert_eq!(merged2, s(&["a", "b1", "b2", "b", "c", "D", "e"]));
    }

    #[test]
    fn rebase_anchors_pure_insert() {
        let base = s(&["a", "b", "c"]);
        // Concurrent edits: insert "x" before "b", and append "1","2" at the end.
        let ins_top = LinePatch {
            start: 1,
            remove: 0,
            old: vec![],
            prev: Some("a".into()),
            next: Some("b".into()),
            lines: s(&["x"]),
        };
        let ins_end = LinePatch {
            start: 3,
            remove: 0,
            old: vec![],
            prev: Some("c".into()),
            next: None,
            lines: s(&["1", "2"]),
        };
        let mut after_top = base.clone();
        apply_blocks(&mut after_top, std::slice::from_ref(&ins_top));
        assert_eq!(after_top, s(&["a", "x", "b", "c"]));
        let rebased = rebase_blocks(&after_top, std::slice::from_ref(&ins_end)).unwrap();
        assert_eq!(rebased[0].start, 4);
        let mut merged = after_top.clone();
        apply_blocks(&mut merged, &rebased);
        assert_eq!(merged, s(&["a", "x", "b", "c", "1", "2"]));
    }

    #[test]
    fn diff_multiple_disjoint_blocks() {
        // Two far-apart edits in one keystroke batch: replace b->B and e->E.
        // They must NOT be fused into a single span covering c,d.
        let old = s(&["a", "b", "c", "d", "e", "f"]);
        let new = s(&["a", "B", "c", "d", "E", "f"]);
        let p = compute_line_patches(&old, &new);
        assert_eq!(p.len(), 2, "two disjoint edits must be two blocks");
        assert_eq!((p[0].start, p[0].remove), (1, 1));
        assert_eq!(p[0].old, s(&["b"]));
        assert_eq!(p[0].lines, s(&["B"]));
        assert_eq!((p[1].start, p[1].remove), (4, 1));
        assert_eq!(p[1].old, s(&["e"]));
        assert_eq!(p[1].lines, s(&["E"]));
        // Applying the blocks reproduces `new`.
        let mut lines = old.clone();
        apply_blocks(&mut lines, &p);
        assert_eq!(lines, new);
    }

    #[test]
    fn diff_three_regions_insert_delete_replace() {
        // Insert "x" at top, delete "d", replace "g"->"G".
        let old = s(&["a", "b", "c", "d", "e", "f", "g"]);
        let new = s(&["x", "a", "b", "c", "e", "f", "G"]);
        let p = compute_line_patches(&old, &new);
        assert_eq!(p.len(), 3);
        // top insertion
        assert_eq!((p[0].start, p[0].remove), (0, 0));
        assert!(p[0].old.is_empty());
        assert_eq!(p[0].lines, s(&["x"]));
        // delete d (start shifted by the +1 inserted above)
        assert_eq!((p[1].start, p[1].remove), (4, 1));
        assert_eq!(p[1].old, s(&["d"]));
        assert!(p[1].lines.is_empty());
        // replace g
        assert_eq!((p[2].start, p[2].remove), (6, 1));
        assert_eq!(p[2].old, s(&["g"]));
        assert_eq!(p[2].lines, s(&["G"]));
        let mut lines = old.clone();
        apply_blocks(&mut lines, &p);
        assert_eq!(lines, new);
    }

    #[test]
    fn rebase_multi_block_onto_concurrent_edit() {
        // User A edits two regions (b->B at 1, e->E at 4) in one patch.
        // User B concurrently edits a THIRD region (c->C at 2). A is behind;
        // rebasing A's two blocks must keep BOTH, independently re-anchored.
        let base = s(&["a", "b", "c", "d", "e", "f"]);
        let a_patch = compute_line_patches(&base, &s(&["a", "B", "c", "d", "E", "f"]));
        assert_eq!(a_patch.len(), 2);
        // B applied first.
        let mut after_b = base.clone();
        apply_blocks(&mut after_b, &[LinePatch {
            start: 2,
            remove: 1,
            old: s(&["c"]),
            prev: Some("b".into()),
            next: Some("d".into()),
            lines: s(&["C"]),
        }]);
        assert_eq!(after_b, s(&["a", "b", "C", "d", "e", "f"]));
        // Rebase A's disjoint blocks onto after_b: both must land.
        let rebased = rebase_blocks(&after_b, &a_patch).unwrap();
        assert_eq!(rebased.len(), 2);
        let mut merged = after_b.clone();
        apply_blocks(&mut merged, &rebased);
        assert_eq!(merged, s(&["a", "B", "C", "d", "E", "f"]));
    }

    #[test]
    fn rebase_onto_folds_all_pending_blocks() {
        // Canonical is behind the user's buffer by two separate edits; a foreign
        // patch arrives in a third region. rebase_onto must fold BOTH pending
        // edits on top of the foreign content.
        let canonical = s(&["a", "b", "c", "d", "e", "f"]);
        let editing = s(&["a", "B", "c", "D", "e", "f"]);
        let pend = compute_line_patches(&canonical, &editing);
        assert_eq!(pend.len(), 2);
        let foreign = s(&["a", "b", "c", "d", "e", "f", "g"]);
        let out = rebase_onto(&foreign, &pend).unwrap();
        assert_eq!(out, s(&["a", "B", "c", "D", "e", "f", "g"]));
    }

    #[test]
    fn rebase_conflict_is_none() {
        let base = s(&["a", "b", "c"]);
        let a = LinePatch {
            start: 1,
            remove: 1,
            old: s(&["b"]),
            prev: Some("a".into()),
            next: Some("c".into()),
            lines: s(&["X"]),
        };
        let mut after_a = base.clone();
        apply_blocks(&mut after_a, std::slice::from_ref(&a));
        // B's block "b" is gone.
        let b = LinePatch {
            start: 1,
            remove: 1,
            old: s(&["b"]),
            prev: Some("a".into()),
            next: Some("c".into()),
            lines: s(&["Y"]),
        };
        assert!(rebase_blocks(&after_a, std::slice::from_ref(&b)).is_none());
    }
}
