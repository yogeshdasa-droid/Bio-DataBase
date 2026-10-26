import { useMemo } from "react";
import { Network } from "lucide-react";
import { PersonListItem } from "../api";
import { T } from "../i18n";

const W = 200; // box width
const H = 30; // box height
const DX = 250; // from one generation to the next
const DY = 40; // from one line to the next

interface Box {
  p: PersonListItem;
  x: number;
  y: number;
  parent: Box | null;
}

/** The Guru Paramparā drawn left to right: guru → disciples. */
export function TreeView({ t, persons, onPerson }: { t: T; persons: PersonListItem[]; onPerson: (id: number) => void }) {
  const { boxes, width, height } = useMemo(() => {
    const ids = new Set(persons.filter((p) => p.tree).map((p) => p.id));
    const kids = new Map<number | null, PersonListItem[]>();
    for (const p of persons) {
      if (!p.tree) continue;
      const parent = p.tree.parent !== null && ids.has(p.tree.parent) ? p.tree.parent : null;
      kids.set(parent, [...(kids.get(parent) ?? []), p]);
    }
    for (const list of kids.values()) list.sort((a, b) => a.tree!.pos - b.tree!.pos);
    const boxes: Box[] = [];
    let line = 0;
    // leaves take the next line; a guru sits in the middle of their disciples
    const place = (p: PersonListItem, depth: number, parent: Box | null): Box => {
      const box: Box = { p, x: depth * DX, y: 0, parent };
      boxes.push(box);
      const children = (kids.get(p.id) ?? []).map((c) => place(c, depth + 1, box));
      box.y = children.length ? (children[0].y + children[children.length - 1].y) / 2 : line++ * DY;
      return box;
    };
    for (const root of kids.get(null) ?? []) place(root, 0, null);
    const width = Math.max(...boxes.map((b) => b.x), 0) + W + 20;
    const height = Math.max(line, 1) * DY + 20;
    return { boxes, width, height };
  }, [persons]);

  const short = (s: string) => (s.length > 28 ? s.slice(0, 27) + "…" : s);

  return (
    <div className="reader">
      <div className="reader-scroll">
        <div className="tree-view">
          <header className="view-head">
            <h1>
              <Network size={22} /> {t.treeView}
            </h1>
            <p className="muted small">{t.treeViewHint}</p>
          </header>
          {boxes.length === 0 ? (
            <p className="muted">{t.paramparaEmpty}</p>
          ) : (
            <div className="tree-canvas">
              <svg width={width} height={height} viewBox={`-10 -10 ${width} ${height}`}>
                {boxes
                  .filter((b) => b.parent)
                  .map((b) => {
                    const a = b.parent!;
                    const x1 = a.x + W;
                    const y1 = a.y + H / 2;
                    const x2 = b.x;
                    const y2 = b.y + H / 2;
                    const mx = (x1 + x2) / 2;
                    return <path key={`l${b.p.id}`} className="tree-link" d={`M${x1},${y1} C${mx},${y1} ${mx},${y2} ${x2},${y2}`} />;
                  })}
                {boxes.map((b) => (
                  <g key={b.p.id} className="tree-node" transform={`translate(${b.x},${b.y})`} onClick={() => onPerson(b.p.id)}>
                    <title>{b.p.name}</title>
                    <rect width={W} height={H} rx={5} />
                    <text x={10} y={H / 2 + 4}>
                      {short(b.p.name)}
                    </text>
                  </g>
                ))}
              </svg>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
