# Bio-Data

Aplicación de escritorio para Windows con **biografías vaiṣṇavas**: fichas de cada santo,
sus textos y búsqueda instantánea. Hermana de [GaudiyaDB](https://github.com/yogeshdasa-droid/GaudiyaDB).

A diferencia de GaudiyaDB, Bio-Data es **autosuficiente**: al importar, el texto, las portadas
y los PDF / Word originales se copian **dentro de la base de datos**. Después las carpetas de
origen ya no hacen falta, y todo se edita desde la app.

## Qué hace

- **Fichas de personas**: nombre, otros nombres, aparición, desaparición, lugar, guru,
  discípulos, obras y notas. Cada ficha reúne todos los textos sobre esa persona
  (p. ej. la nota de Obsidian y los volúmenes de Calibre de *Jiva Gosvami* en una sola ficha).
- **Unir fichas** cuando dos nombres son la misma persona; la app sugiere nombres parecidos.
- **Importar** carpetas o archivos `.md`, `.txt`, `.pdf`, `.docx`, `.doc` (Word / WordPerfect
  antiguos) y `.html`. Reconoce bibliotecas de Calibre (PDF + portada + `metadata.opf`).
  Las copias idénticas se omiten; volver a importar solo añade lo nuevo.
- Convierte la fuente antigua **Balaram** (`Kåñëa`) a IAST (`Kṛṣṇa`).
- **Búsqueda** sin diacríticos y con variantes (`krishna` = `kṛṣṇa`), frases exactas,
  `palabra*`, `-excluir`, `OR`; filtros por colección, carpeta, texto o persona.
- **Lector** con resaltado, marcadores y visor del **PDF original** guardado.
- **Edición** del texto y los datos; **papelera** con recuperación; **copia de seguridad**
  de todo en un solo archivo.
- Español / inglés, tema claro / oscuro.

## Desarrollo

Requiere Node.js y Rust.

```powershell
npm install
npm run app:dev     # modo desarrollo
npx tsc --noEmit    # comprobar tipos
cd src-tauri; cargo test
npm run app:build   # instalador en src-tauri/target/release/bundle/
```

La base de datos está en `%APPDATA%\com.whispers.biodata\biodata.db`.

Hecho con Tauri 2, Rust (SQLite FTS5) y React.
