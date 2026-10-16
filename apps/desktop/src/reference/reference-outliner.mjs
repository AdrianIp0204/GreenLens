/** Display indexing only. Geometry, identities and parent relationships were admitted by Rust. */
export const OUTLINER_PAGE_SIZE = 100;
export function indexOccurrences(occurrences) {
  const byId = new Map(occurrences.map(row => [row.id, row]));
  const children = new Map();
  const roots = [];
  const meshUses = new Map();
  for (const row of occurrences) {
    if (row.parent_id === null) roots.push(row.id);
    else { const ids = children.get(row.parent_id) ?? []; ids.push(row.id); children.set(row.parent_id, ids); }
    if (row.source_mesh_index !== null) meshUses.set(row.source_mesh_index, (meshUses.get(row.source_mesh_index) ?? 0) + 1);
  }
  return { byId, children, roots, meshUses };
}
export function occurrenceAncestors(index, id) {
  const result = [], seen = new Set();
  let row = index.byId.get(id);
  while (row && result.length < 64 && !seen.has(row.id)) {
    seen.add(row.id); result.push(row); row = row.parent_id === null ? undefined : index.byId.get(row.parent_id);
  }
  return result.reverse();
}
export function outlinerRows(index, query, collapsed) {
  const needle = query.trim().toLocaleLowerCase();
  const rows = [], stack = index.roots.map(id => ({ id, depth: 0 })).reverse(), seen = new Set();
  while (stack.length) {
    const { id, depth } = stack.pop(), row = index.byId.get(id);
    if (!row || seen.has(id)) continue;
    seen.add(id);
    const ancestors = needle ? occurrenceAncestors(index, id) : [];
    const matches = !needle || [row.id, String(row.node_index), ...ancestors.map(item => item.label)].join(' ').toLocaleLowerCase().includes(needle);
    if (matches) rows.push({ occurrence: row, depth, hasChildren: Boolean(index.children.get(id)?.length) });
    if (needle || !collapsed.has(id)) for (const child of [...(index.children.get(id) ?? [])].reverse()) stack.push({ id: child, depth: depth + 1 });
  }
  return rows;
}
