import { getVersion } from "@tauri-apps/api/app";

/** Where new versions are published. */
const LATEST = "https://api.github.com/repos/yogeshdasa-droid/Bio-DataBase/releases/latest";

export interface Update {
  version: string;
  /** the installer (or the release page when it has none) */
  url: string;
  page: string;
}

/** "0.10.2" > "0.9.9" */
function newer(a: string, b: string): boolean {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d > 0;
  }
  return false;
}

/** The latest published version when it is newer than this program; null otherwise. */
export async function checkUpdate(): Promise<Update | null> {
  const [current, res] = await Promise.all([getVersion(), fetch(LATEST, { headers: { Accept: "application/vnd.github+json" } })]);
  if (!res.ok) throw new Error(`GitHub: ${res.status}`);
  const r = (await res.json()) as { tag_name: string; html_url: string; assets: { name: string; browser_download_url: string }[] };
  const version = r.tag_name.replace(/^v/, "");
  if (!newer(version, current)) return null;
  const setup = r.assets.find((a) => a.name.toLowerCase().endsWith("-setup.exe")) ?? r.assets.find((a) => a.name.endsWith(".exe"));
  return { version, url: setup?.browser_download_url ?? r.html_url, page: r.html_url };
}
