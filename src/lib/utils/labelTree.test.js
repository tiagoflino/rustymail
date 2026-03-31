import { describe, it, expect } from 'vitest';
import { buildLabelTree } from './labelTree';

/** @param {string} name @param {string} [id] */
function makeLabel(name, id) {
  return { id: id || name, name, type: 'user', unread_count: 0, bgColor: null };
}

describe('buildLabelTree', () => {
  it('returns empty array for empty input', () => {
    const tree = buildLabelTree([]);
    expect(tree).toEqual([]);
  });

  it('returns flat labels as top-level nodes', () => {
    const labels = [makeLabel('Work'), makeLabel('Personal'), makeLabel('Finance')];
    const tree = buildLabelTree(labels);
    expect(tree).toHaveLength(3);
    expect(tree.map(n => n.name)).toEqual(['Finance', 'Personal', 'Work']); // sorted
    expect(tree[0].children).toEqual([]);
    expect(tree[0].label?.name).toBe('Finance');
  });

  it('nests labels with / separator', () => {
    const labels = [
      makeLabel('Work/Projects/Active', 'l1'),
      makeLabel('Work/Projects/Archive', 'l2'),
      makeLabel('Work/Meetings', 'l3'),
    ];
    const tree = buildLabelTree(labels);

    // Top level: just "Work"
    expect(tree).toHaveLength(1);
    expect(tree[0].name).toBe('Work');
    expect(tree[0].fullPath).toBe('Work');
    expect(tree[0].label).toBeNull(); // intermediate node, no label for "Work" itself

    // Second level: "Meetings" and "Projects"
    expect(tree[0].children).toHaveLength(2);
    expect(tree[0].children[0].name).toBe('Meetings');
    expect(tree[0].children[0].label?.id).toBe('l3');
    expect(tree[0].children[1].name).toBe('Projects');
    expect(tree[0].children[1].label).toBeNull(); // intermediate

    // Third level: "Active" and "Archive"
    expect(tree[0].children[1].children).toHaveLength(2);
    expect(tree[0].children[1].children[0].name).toBe('Active');
    expect(tree[0].children[1].children[0].label?.id).toBe('l1');
    expect(tree[0].children[1].children[1].name).toBe('Archive');
    expect(tree[0].children[1].children[1].label?.id).toBe('l2');
  });

  it('handles parent label that is also a real label', () => {
    const labels = [
      makeLabel('Work', 'work_label'),
      makeLabel('Work/Projects', 'projects_label'),
    ];
    const tree = buildLabelTree(labels);

    expect(tree).toHaveLength(1);
    expect(tree[0].name).toBe('Work');
    expect(tree[0].label?.id).toBe('work_label'); // parent has its own label
    expect(tree[0].children).toHaveLength(1);
    expect(tree[0].children[0].name).toBe('Projects');
    expect(tree[0].children[0].label?.id).toBe('projects_label');
  });

  it('mixes flat and nested labels correctly', () => {
    const labels = [
      makeLabel('Newsletters'),
      makeLabel('Work/Projects'),
      makeLabel('Personal/Finance'),
      makeLabel('Receipts'),
    ];
    const tree = buildLabelTree(labels);

    // Top level should have 4 nodes: Newsletters, Personal, Receipts, Work (sorted)
    expect(tree).toHaveLength(4);
    expect(tree.map(n => n.name)).toEqual(['Newsletters', 'Personal', 'Receipts', 'Work']);

    // Flat labels have their label attached and no children
    expect(tree[0].label?.name).toBe('Newsletters');
    expect(tree[0].children).toHaveLength(0);

    // Nested labels have children
    expect(tree[1].name).toBe('Personal');
    expect(tree[1].children).toHaveLength(1);
    expect(tree[1].children[0].name).toBe('Finance');
  });

  it('handles deeply nested labels', () => {
    const labels = [
      makeLabel('A/B/C/D/E', 'deep'),
    ];
    const tree = buildLabelTree(labels);

    expect(tree).toHaveLength(1);
    expect(tree[0].name).toBe('A');
    expect(tree[0].label).toBeNull();

    let node = tree[0];
    for (const expected of ['B', 'C', 'D']) {
      expect(node.children).toHaveLength(1);
      node = node.children[0];
      expect(node.name).toBe(expected);
      expect(node.label).toBeNull();
    }
    // Leaf
    expect(node.children).toHaveLength(1);
    expect(node.children[0].name).toBe('E');
    expect(node.children[0].label?.id).toBe('deep');
    expect(node.children[0].children).toHaveLength(0);
  });

  it('preserves fullPath for each node', () => {
    const labels = [
      makeLabel('Folders/1_PERSONAL/1_LANGUAGE', 'lang'),
    ];
    const tree = buildLabelTree(labels);

    expect(tree[0].fullPath).toBe('Folders');
    expect(tree[0].children[0].fullPath).toBe('Folders/1_PERSONAL');
    expect(tree[0].children[0].children[0].fullPath).toBe('Folders/1_PERSONAL/1_LANGUAGE');
  });

  it('handles single label', () => {
    const labels = [makeLabel('Solo')];
    const tree = buildLabelTree(labels);
    expect(tree).toHaveLength(1);
    expect(tree[0].name).toBe('Solo');
    expect(tree[0].label?.name).toBe('Solo');
  });
});
