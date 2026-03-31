export interface LabelTreeNode {
  label: Record<string, any> | null;
  name: string;
  fullPath: string;
  children: LabelTreeNode[];
}

export function buildLabelTree(labels: { name: string; [key: string]: any }[]): LabelTreeNode[] {
  const root: LabelTreeNode[] = [];
  const sorted = [...labels].sort((a, b) => a.name.localeCompare(b.name));

  for (const label of sorted) {
    const parts = label.name.split('/');
    let currentLevel = root;
    let pathSoFar = '';

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      pathSoFar = pathSoFar ? `${pathSoFar}/${part}` : part;
      const isLast = i === parts.length - 1;

      let existing = currentLevel.find(n => n.name === part && n.fullPath === pathSoFar);
      if (!existing) {
        existing = {
          label: isLast ? label : null,
          name: part,
          fullPath: pathSoFar,
          children: [],
        };
        currentLevel.push(existing);
      } else if (isLast) {
        existing.label = label;
      }

      currentLevel = existing.children;
    }
  }

  return root;
}
