import { invoke } from "@tauri-apps/api/core";
import type { Term } from "./fold";

export type Sort = "relevance" | "title" | "person";

export interface SearchParams {
  query: string;
  titlesOnly: boolean;
  prefix: boolean;
  sort: Sort;
  collections: string[];
  dir: string | null;
  docId: number | null;
  personId: number | null;
  offset: number;
  limit: number;
}

export interface Hit {
  sectionId: number;
  docId: number;
  ordinal: number;
  sections: number;
  heading: string | null;
  title: string;
  collection: string;
  dir: string;
  personId: number | null;
  person: string | null;
  snippet: string;
  hits: number;
}

export interface SearchResult {
  total: number;
  hits: Hit[];
  facets: { name: string; count: number }[];
  terms: Term[];
  millis: number;
  error: string | null;
}

export interface Status {
  docs: number;
  sections: number;
  persons: number;
  trash: number;
  filesMb: number;
  busy: boolean;
  dbPath: string;
  dbSize: number;
}

export interface DocListItem {
  id: number;
  dir: string;
  title: string;
  personId: number | null;
}

export interface DocInfo {
  id: number;
  dir: string;
  collection: string;
  title: string;
  date: string | null;
  tags: string[];
  size: number;
  sections: number;
  kind: string;
  source: string;
  personId: number | null;
  person: string | null;
  /** file name of the original PDF / Word kept in the database */
  original: string | null;
  hasCover: boolean;
  created: string;
  modified: string;
}

export interface TocEntry {
  ordinal: number;
  heading: string | null;
}

export interface SectionText {
  id: number;
  ordinal: number;
  heading: string | null;
  text: string;
}

export interface PersonFields {
  name: string;
  aka: string;
  appearance: string;
  disappearance: string;
  place: string;
  guru: string;
  disciples: string;
  works: string;
  notes: string;
}

export const EMPTY_PERSON: PersonFields = {
  name: "",
  aka: "",
  appearance: "",
  disappearance: "",
  place: "",
  guru: "",
  disciples: "",
  works: "",
  notes: "",
};

export interface PersonListItem {
  id: number;
  name: string;
  aka: string;
  docs: number;
  coverDoc: number | null;
  hasPhoto: boolean;
}

export interface PersonDoc {
  id: number;
  title: string;
  collection: string;
  kind: string;
  size: number;
  hasCover: boolean;
}

export interface Person extends PersonFields {
  id: number;
  created: string;
  modified: string;
  hasPhoto: boolean;
  docs: PersonDoc[];
}

export interface DocMeta {
  title: string;
  date: string;
  collection: string;
  personId: number | null;
}

export type LinkTarget = { kind: "doc"; id: number } | { kind: "person"; id: number } | { kind: "none" };

export interface Bookmark {
  id: number;
  docId: number | null;
  ordinal: number;
  title: string;
  heading: string | null;
  note: string;
  created: string;
}

export interface Trashed {
  id: number;
  title: string;
  collection: string;
  person: string | null;
  deleted: string;
}

export interface ImportSource {
  path: string;
  collection: string;
}

export interface DefaultSource extends ImportSource {
  exists: boolean;
}

export interface ImportOptions {
  sources: ImportSource[];
  personPerFile: boolean;
  personId: number | null;
}

export interface ImportProgress {
  phase: "scan" | "import" | "fts" | "done";
  done: number;
  total: number;
}

export interface ImportSummary {
  added: number;
  duplicates: number;
  persons: number;
  failed: string[];
  millis: number;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  importSources: (options: ImportOptions) => invoke<void>("import_sources", { options }),
  rebuildSearch: () => invoke<void>("rebuild_search"),
  defaultSources: () => invoke<DefaultSource[]>("default_sources"),
  pathKinds: (paths: string[]) => invoke<("dir" | "text" | "other")[]>("path_kinds", { paths }),

  search: (params: SearchParams) => invoke<SearchResult>("search", { params }),
  listDocs: () => invoke<DocListItem[]>("list_docs"),
  getDoc: (id: number) => invoke<{ doc: DocInfo; toc: TocEntry[] }>("get_doc", { id }),
  getSections: (docId: number, from: number, to: number) => invoke<SectionText[]>("get_sections", { docId, from, to }),
  resolveLink: (target: string) => invoke<LinkTarget>("resolve_link", { target }),

  listPersons: () => invoke<PersonListItem[]>("list_persons"),
  getPerson: (id: number) => invoke<Person>("get_person", { id }),
  searchPersons: (query: string) => invoke<number[]>("search_persons", { query }),
  newPerson: (fields: PersonFields) => invoke<number>("new_person", { fields }),
  updatePerson: (id: number, fields: PersonFields) => invoke<void>("update_person", { id, fields }),
  deletePerson: (id: number) => invoke<void>("delete_person", { id }),
  mergePersons: (into: number, from: number[]) => invoke<void>("merge_persons", { into, from }),
  setPersonPhoto: (id: number, path: string) => invoke<void>("set_person_photo", { id, path }),
  removePersonPhoto: (id: number) => invoke<void>("remove_person_photo", { id }),

  createDoc: (personId: number | null, title: string, collection: string) =>
    invoke<number>("create_doc", { personId, title, collection }),
  saveSection: (docId: number, ordinal: number, text: string) => invoke<void>("save_section", { docId, ordinal, text }),
  updateDoc: (docId: number, meta: DocMeta) => invoke<void>("update_doc", { docId, meta }),
  renameFolder: (dir: string, name: string) => invoke<void>("rename_folder", { dir, name }),
  trashDoc: (docId: number) => invoke<void>("trash_doc", { docId }),
  restoreDoc: (docId: number) => invoke<void>("restore_doc", { docId }),
  listTrash: () => invoke<Trashed[]>("list_trash"),
  purgeDocs: (ids: number[]) => invoke<void>("purge_docs", { ids }),

  listBookmarks: () => invoke<Bookmark[]>("list_bookmarks"),
  addBookmark: (docId: number, ordinal: number, note: string) => invoke<number>("add_bookmark", { docId, ordinal, note }),
  updateBookmark: (id: number, note: string) => invoke<void>("update_bookmark", { id, note }),
  deleteBookmark: (id: number) => invoke<void>("delete_bookmark", { id }),

  saveOriginal: (docId: number, path: string) => invoke<void>("save_original", { docId, path }),
  openOriginal: (docId: number) => invoke<void>("open_original", { docId }),
  exportDoc: (docId: number, path: string) => invoke<void>("export_doc", { docId, path }),
  backup: (path: string) => invoke<void>("backup", { path }),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
};

/** URL of a file stored in the database (cover image, original PDF, or a record's picture by person id). */
export function fileUrl(role: "cover" | "original" | "photo", docId: number): string {
  return navigator.userAgent.includes("Windows") ? `http://bio.localhost/${role}/${docId}` : `bio://localhost/${role}/${docId}`;
}

/** Stable pastel hue per collection name. */
export function hueOf(name: string): number {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return (h * 137) % 360;
}
