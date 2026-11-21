//! Drop stale `NodeId` references from Blitz paint and stacking caches after DOM removal.
//!
//! A mutation can drop a subtree while a surviving ancestor still lists its ids in
//! `paint_children`, a stacking-context entry, or a hoisted-contribution cache. Blitz
//! indexes those lists by `NodeId` while hit testing and painting, so the next frame
//! panics with `invalid SlotMap key`. Pruning drops only ids that no longer resolve, so
//! live paint order is unchanged.

use std::collections::HashSet;

use blitz_dom::{BaseDocument, HoistedPaintChild};
use blitz_traits::node_id::NodeId;

/// Remove paint and stacking list entries whose nodes were dropped.
///
/// Returns how many stale ids were removed. Call it after a mutation that drops nodes
/// and before the next `resolve`, hit test or paint.
pub(crate) fn prune_stale_paint_children(doc: &mut BaseDocument) -> usize {
    let root = doc.root_node().id;
    prune_at(doc, root, &mut HashSet::new())
}

fn alive(doc: &BaseDocument, id: NodeId) -> bool {
    doc.get_node(id).is_some()
}

fn retain_alive_ids(ids: &mut Vec<NodeId>, doc: &BaseDocument) -> usize {
    let before = ids.len();
    ids.retain(|id| alive(doc, *id));
    before - ids.len()
}

fn retain_alive_entries(entries: &mut Vec<HoistedPaintChild>, doc: &BaseDocument) -> usize {
    let before = entries.len();
    entries.retain(|entry| alive(doc, entry.node_id));
    before - entries.len()
}

/// One node's id lists, copied out so the node can be borrowed mutably for write-back.
struct NodeLists {
    dom_children: Vec<NodeId>,
    layout_children: Option<Vec<NodeId>>,
    hoisted: Vec<NodeId>,
    paint_children: Option<Vec<NodeId>>,
    sc_cache: Vec<HoistedPaintChild>,
    sc_children: Option<Vec<HoistedPaintChild>>,
    anonymous: Vec<NodeId>,
}

fn snapshot_lists(doc: &BaseDocument, node_id: NodeId) -> Option<NodeLists> {
    let node = doc.get_node(node_id)?;
    Some(NodeLists {
        dom_children: node.children.iter().copied().collect(),
        layout_children: node
            .layout_children
            .borrow()
            .as_ref()
            .map(|ids| ids.to_vec()),
        hoisted: node.hoisted_children.borrow().to_vec(),
        paint_children: node
            .paint_children
            .borrow()
            .as_ref()
            .map(|ids| ids.to_vec()),
        sc_cache: node.sc_contribution_cache.borrow().to_vec(),
        sc_children: node
            .stacking_context
            .as_ref()
            .map(|sc| sc.children.to_vec()),
        anonymous: node.anonymous_blocks.to_vec(),
    })
}

fn prune_at(doc: &mut BaseDocument, node_id: NodeId, seen: &mut HashSet<NodeId>) -> usize {
    if !seen.insert(node_id) {
        return 0;
    }
    let Some(lists) = snapshot_lists(doc, node_id) else {
        return 0;
    };

    let mut removed = 0;
    let mut walk = lists.dom_children;

    if let Some(mut ids) = lists.layout_children {
        let n = retain_alive_ids(&mut ids, doc);
        removed += n;
        walk.extend(ids.iter().copied());
        if n > 0
            && let Some(node) = doc.get_node_mut(node_id)
        {
            *node.layout_children.borrow_mut() = Some(ids.into());
        }
    }

    let mut hoisted = lists.hoisted;
    let n = retain_alive_ids(&mut hoisted, doc);
    removed += n;
    walk.extend(hoisted.iter().copied());
    if n > 0
        && let Some(node) = doc.get_node_mut(node_id)
    {
        *node.hoisted_children.borrow_mut() = hoisted.into();
    }

    if let Some(mut ids) = lists.paint_children {
        let n = retain_alive_ids(&mut ids, doc);
        removed += n;
        walk.extend(ids.iter().copied());
        if n > 0
            && let Some(node) = doc.get_node_mut(node_id)
        {
            *node.paint_children.borrow_mut() = Some(ids.into());
        }
    }

    let mut cache = lists.sc_cache;
    let n = retain_alive_entries(&mut cache, doc);
    removed += n;
    walk.extend(cache.iter().map(|entry| entry.node_id));
    if n > 0
        && let Some(node) = doc.get_node_mut(node_id)
    {
        *node.sc_contribution_cache.borrow_mut() = cache.into();
    }

    if let Some(mut entries) = lists.sc_children {
        let n = retain_alive_entries(&mut entries, doc);
        removed += n;
        walk.extend(entries.iter().map(|entry| entry.node_id));
        if n > 0
            && let Some(node) = doc.get_node_mut(node_id)
            && let Some(sc) = node.stacking_context.as_mut()
        {
            sc.children = entries.into();
            sc.sort();
        }
    }

    let mut anonymous = lists.anonymous;
    let n = retain_alive_ids(&mut anonymous, doc);
    removed += n;
    walk.extend(anonymous.iter().copied());
    if n > 0
        && let Some(node) = doc.get_node_mut(node_id)
    {
        node.anonymous_blocks = anonymous.into();
    }

    for child_id in walk {
        if alive(doc, child_id) {
            removed += prune_at(doc, child_id, seen);
        }
    }

    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use blitz_dom::DocumentConfig;

    fn hoisted_from_any_context(doc: &BaseDocument, target: NodeId) -> bool {
        let mut stack = vec![doc.root_node().id];
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(node) = doc.get_node(id) else {
                continue;
            };
            if node
                .stacking_context
                .as_ref()
                .is_some_and(|sc| sc.children.iter().any(|entry| entry.node_id == target))
            {
                return true;
            }
            stack.extend(node.children.iter().copied());
        }
        false
    }

    #[test]
    fn prune_drops_only_dead_ids() {
        let html = concat!(
            "<!DOCTYPE html><html><head><style>",
            "#p { position: relative; width: 100px; height: 100px; }",
            "#c { position: absolute; z-index: 1; width: 50px; height: 50px; }",
            "</style></head><body>",
            "<div id=\"p\"><div id=\"c\">child</div><div id=\"keep\">k</div></div>",
            "</body></html>"
        );
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        doc.resolve(0.0);
        let parent = doc.get_element_by_id("p").expect("parent");
        let child = doc.get_element_by_id("c").expect("child");
        let keep = doc.get_element_by_id("keep").expect("keep");
        assert!(
            hoisted_from_any_context(&doc, child),
            "expected the z-indexed child hoisted into a live stacking context"
        );

        doc.mutate().remove_and_drop_node(child);
        assert!(prune_stale_paint_children(&mut doc) >= 1);

        assert!(
            !hoisted_from_any_context(&doc, child),
            "dead stacking entry survived the prune"
        );
        let parent_node = doc.get_node(parent).expect("parent node");
        assert!(
            !parent_node
                .paint_children
                .borrow()
                .as_ref()
                .is_some_and(|ids| ids.contains(&child)),
            "dead paint child survived the prune"
        );
        assert!(
            parent_node
                .paint_children
                .borrow()
                .as_ref()
                .is_some_and(|ids| ids.contains(&keep)),
            "live paint child was pruned"
        );
        doc.resolve(0.0);
        let _ = doc.hit(10.0, 10.0);
    }
}
