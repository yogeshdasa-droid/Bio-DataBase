# Bio-Data — guía para continuar el proyecto

Documento para retomar el trabajo en el futuro (una persona o una sesión nueva de Claude).
Resume qué es la app, cómo está hecha, qué reglas no hay que romper y qué queda pendiente.
Última actualización: **26-10-2026** (versión **0.2.0**).

> **Para retomar con Claude**, basta con abrir esta carpeta y decir algo como:
> «Lee `DESARROLLO.md` y continuamos con Bio-Data: quiero cambiar…».

---

## 1. Qué es

Aplicación de escritorio para **Windows** con **biografías vaiṣṇavas**: una ficha por cada santo
o devoto, con todos los textos sobre esa persona y búsqueda instantánea.

- Hecha con **Tauri 2** (ventana WebView2), **Rust** (SQLite + búsqueda FTS5) y **React + TypeScript**.
- Repositorio: https://github.com/yogeshdasa-droid/Bio-DataBase (público). Rama `main`.
- Hermana de **GaudiyaDB** (`G:\Whispers\W-DataBase`), de la que se copió la base del código,
  pero es un **proyecto independiente**: no se toca nada de esa carpeta desde aquí, y las dos
  apps pueden estar abiertas a la vez.

### La idea clave: la base de datos ES la biblioteca

Al importar, el texto, las portadas y los PDF / Word originales se copian **dentro** de la base
de datos. Después las carpetas de origen no se vuelven a leer ni se modifican: todo se edita en
la app. Un solo archivo lo contiene todo (y la copia de seguridad es ese archivo).

- Base de datos: `%APPDATA%\com.whispers.biodata\biodata.db` (≈ 240 MB).
- Copia hecha antes de limpiar las notas (25-10-2026):
  `%APPDATA%\com.whispers.biodata\biodata antes de limpiar notas.db` (se puede borrar).
- La base de datos **no** está en GitHub (es del usuario y es grande).

### Llevar la app a otro PC

El instalador trae **solo el programa, no los datos**. Los datos están en el archivo
`biodata.db`. Para tenerlo todo en otro PC:

1. En este PC: Ajustes › General › **Copia de seguridad** (o copiar `%APPDATA%\com.whispers.biodata\biodata.db`
   con la app cerrada).
2. En el otro PC: instalar Bio-Data, **abrirlo una vez y cerrarlo** (crea la carpeta).
3. Copiar el archivo a `%APPDATA%\com.whispers.biodata\biodata.db` en el otro PC (sustituyendo el que
   haya; si hay `biodata.db-wal` / `-shm`, borrarlos) y abrir la app.

Si no se copia, la primera vez intenta importar las carpetas de `DEFAULT_SOURCES`
(`G:\Whispers\Data Bases\...`); si no existen, empieza vacía. Idea pendiente: un botón
«Restaurar copia de seguridad» en Ajustes.

---

## 2. Reglas que no hay que romper

1. **Nunca borrar datos del usuario.** Tablas de datos: `persons`, `docs`, `sections`, `files`,
   `bookmarks`, `meta`, `person_photos`, `person_tree`. Si cambia su estructura → subir `DATA_VERSION` en
   `src-tauri/src/db.rs` y escribir una migración que conserve todo. Un programa viejo se niega
   a abrir una base de datos de formato más nuevo (para no estropearla).
2. Tablas **derivadas** (se pueden reconstruir): `sections_fts`, `docs_fts`, `persons_fts`.
   Si cambian las reglas de búsqueda / plegado → subir `FTS_VERSION` (se reconstruyen solas al abrir).
3. `src-tauri/src/fold.rs` y `src/fold.ts` hacen **lo mismo** (quitar diacríticos y variantes:
   `krishna` = `krsna` = `kṛṣṇa`). Si se cambia uno, cambiar el otro igual.
4. Antes de un cambio masivo sobre los textos guardados, **hacer copia** de la base de datos
   (`VACUUM INTO`, como en `main.rs`) y dejarlo apuntado en `meta` para que se haga una sola vez.
5. Papelera = `docs.deleted` (no borra). «Borrar para siempre» solo desde Ajustes › Papelera.

---

## 3. Cómo trabajar

Requisitos: Node.js y Rust (en este equipo ya están). En PowerShell, si no encuentra las
herramientas, recargar el PATH:

```powershell
$env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')
npm install            # solo la primera vez en un equipo nuevo
npm run app:dev        # abrir la app en modo pruebas (puerto 1421)
npx tsc --noEmit       # comprobar errores de TypeScript
cd src-tauri; cargo test   # pruebas automáticas de Rust (14)
npm run app:build      # generar el instalador
```

- El equipo es lento (Kaspersky): la primera compilación completa tarda ~6–10 min;
  después, tras cambiar solo nuestro código, entre segundos y 2 min.
- `npm run app:dev` usa **la misma base de datos real** que la app instalada: no tener las
  dos abiertas a la vez si se van a hacer cambios.
- Importación real a una base de datos de prueba (no toca la buena):
  `$env:BIO_DB = "C:\ruta\test.db"; cargo test real_import -- --ignored --nocapture`

### Probar la interfaz desde Claude (capturas y clics)

Arrancar la app con la depuración de WebView2 activada y manejarla por CDP con un script de Node
(`Runtime.evaluate` para pulsar / leer, `Page.captureScreenshot` para ver la pantalla):

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9333'
npm run app:dev
# luego: http://127.0.0.1:9333/json → webSocketDebuggerUrl de la página localhost:1421
```

Desde la página se puede llamar al backend con `window.__TAURI_INTERNALS__.invoke("comando", {...})`.
Para probar cosas que modifican datos, crear fichas de prueba (p. ej. «Zzprueba…») y borrarlas al final.

### Publicar una versión nueva

1. Subir el número de versión en `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`.
2. `npm run app:build` → `src-tauri/target/release/bundle/nsis/Bio-Data_X.Y.Z_x64-setup.exe`
   (y el programa suelto en `src-tauri/target/release/bio-data.exe`).
3. Commit + push, y luego la release en GitHub:
   `gh release create vX.Y.Z <instalador> <exe renombrado> --target main --title "Bio-Data X.Y.Z" --notes-file notas.md`

---

## 4. Estructura del código

```
src-tauri/src/                       (Rust: datos y lógica)
  main.rs     comandos que llama la interfaz; protocolo bio:// (portadas, PDF, fotos);
              importación inicial (DEFAULT_SOURCES); trabajos en segundo plano;
              limpieza única de notas al arrancar
  db.rs       esquema de tablas, DATA_VERSION / FTS_VERSION
  store.rs    escritura: textos, secciones, fichas (person_key, unir), papelera, fotos,
              clean_callouts, rebuild_fts
  import.rs   importador: Calibre (opf + portada + pdf), notas de Obsidian en carpetas A–Z
              (= una ficha cada una), duplicados por huella, «Rupa - v.1» → ficha «Rupa…»
  extract.rs  md / txt / pdf / docx / html y .doc antiguos (texto legible del binario;
              Word por COM se cuelga con estos archivos); fuente Balaram → IAST
  search.rs   búsqueda: palabras (Y), "frase exacta" (también “ ” « »), pal*, -excluir, OR;
              fichas, enlaces [[...]], marcadores
  text.rs     decodificación, división en secciones (>24 KB se corta cada ~6–16 KB),
              fragmentos de resultados, strip_callouts
  fold.rs     plegado de letras (ver regla 3)

src/                                 (React: la interfaz)
  App.tsx           estado general, barra superior, atrás/adelante, menú contextual, diálogos
  api.ts            tipos y llamadas al backend; fileUrl() para bio://
  i18n.ts           todos los textos en español e inglés
  markdown.ts       renderMarkdown (estilos Literata/Inter) y renderSource (estilo «Editor»)
  personLinks.ts    nombres de fichas dentro de los textos → enlaces
  fold.ts           plegado (igual que fold.rs) para resaltar en el lector
  styles.css        todos los estilos (tema claro/oscuro con variables en :root)
  components/
    Sidebar.tsx     pestañas Personas / Biblioteca / Marcadores / Historial
    PersonView.tsx  la ficha: imagen, datos, textos (uno → se muestra dentro), parecidas
    Reader.tsx      lector de un texto (por secciones) y visor del PDF original; SectionView
    Dialogs.tsx     editar texto, ficha, unir fichas, importar, mandar a la papelera
    Settings.tsx    Ajustes: General, Carpetas de origen, Papelera, Ayuda
    Home.tsx, Results.tsx, ContextMenu.tsx, ui.tsx
```

Direcciones internas de archivos (las sirve `serve_file` en `main.rs`):
`http://bio.localhost/cover/<docId>`, `/original/<docId>`, `/photo/<personId>`.

---

## 5. Qué hace hoy (v0.2.0)

- **Fichas**: nombre, otros nombres, aparición, desaparición, lugar, guru, discípulos, obras,
  notas y **una imagen** (pinchar en el cuadro grande; se guarda en `person_photos`).
- **Personas en dos grupos**: **Guru Paramparā** (árbol guru → discípulos, plegable) y
  **Otros vaiṣṇavas** (por orden alfabético). Se ordena **arrastrando con el ratón**: encima de una
  ficha = su discípulo; en el borde de arriba / abajo = antes / después; a «Otros vaiṣṇavas» = sacarla
  (sus discípulos suben a su lugar). También en el menú del botón derecho. Tabla `person_tree`
  (DATA_VERSION 3); al unir o borrar fichas el árbol se mantiene. La primera vez se rellena sola con
  la línea habitual desde Śrī Caitanya (`PARAMPARA` en `store.rs`, meta `parampara_seeded`).
  El arrastre está hecho a mano (mousedown/mousemove): el arrastre del navegador no funciona porque
  Tauri lo usa para importar archivos.
- **Estilo de la app** (Ajustes › General): **«VS Code» por defecto** (colores Dark/Light Modern de
  VS Code, letra Segoe UI, esquinas rectas, sin degradados) o «Bio-Data (anterior)». Es solo CSS:
  bloque `data-skin="vscode"` al final de `styles.css`; `index.html` lo aplica antes de pintar.
- **Una ficha con un solo texto** lo muestra directamente; con varios, tarjetas.
- **Unir fichas** (sugiere nombres parecidos; nunca marca nada solo). Al unir, los textos y la
  imagen pasan a la ficha que queda y los nombres van a «otros nombres».
- **Nombres como enlaces**: si un texto menciona a alguien con ficha, el nombre se subraya y
  lleva a su ficha. Solo nombres de **2 o más palabras** (los de una, como «Yamuna» o «Rupa»,
  darían enlaces falsos), la primera vez que aparecen, y nunca la propia ficha del texto.
- **Lector** con tres estilos (Ajustes): **«Editor» por defecto** (Markdown visible como en
  VS Code: letra de ancho fijo, `##` azul, `código` naranja), Literata e Inter.
  Resaltado de la búsqueda, marcadores, índice de secciones, visor del **PDF original**.
- **Atrás / Adelante**: flechas de la barra superior, `Alt+←/→` y botones laterales del ratón;
  al volver a un texto se abre en la sección que se estaba leyendo.
- **Notas de Obsidian limpias**: sin `> [!example]` ni `>` (`text::strip_callouts`): el título del
  callout pasa a `## NOMBRE`. Se aplica al importar y se aplicó una vez a todo lo ya importado.
- **Importar** carpetas o archivos (también arrastrando a la ventana), **editar** textos y datos,
  **papelera**, **copia de seguridad**, español / inglés, tema claro / oscuro, ventana estrecha.

### Datos importados la primera vez (en este orden)
`Obsidian\Biografias` (1.810 notas en carpetas A–Z), `Vaisnava's Biographies` (Calibre, 388 PDF),
`Calibre\Yogi Library\Vaisnava's Biographies` (6), `Obsidian\Goswami Grantha\BIOGRAPH` (103 .DOC).
Resultado: 2.301 textos, 1.883 fichas, 7 duplicados omitidos, 0 fallos.

---

## 6. El usuario y cómo trabajar con él

- Habla **español** y **no es programador**: explicar en lenguaje claro, sin jerga.
- Le gusta leer el texto **tal como se ve en VS Code** (estilo «Editor») y **evitar clics**
  innecesarios (por eso la ficha con un texto lo muestra directamente).
- **No subir a GitHub hasta que lo haya revisado.** Commit / push / release solo cuando lo pida.
- Prueba la app a la vez que se trabaja: si se automatiza la interfaz, puede haber clics suyos
  mezclados; avisarle.

---

## 7. Pendientes e ideas

- Título de algunos .DOC con basura al principio («ÃTHE ACCEPTANCE…»).
- 9 textos vacíos (notas vacías y 1 PDF escaneado sin texto).
- Rellenar las fichas (fechas, guru, discípulos…), quizá a partir de los textos.
- Exportar todo a Markdown.
- Muchas fichas duplicadas por variantes de nombre («Rupa Gosvami» / «Rupa Goswami,Srila»):
  repasarlas con «Unir fichas».
- Enlaces de nombres: ahora se calculan en la pantalla cada vez; si algún día va lento con textos
  muy largos, se podría calcular en Rust.

---

## 8. Historial

- **26-10-2026 — v0.2.0**: estilo de la app «VS Code» (por defecto; el anterior sigue en Ajustes).
  Personas en «Guru Paramparā» (árbol, arrastrar y soltar) y «Otros vaiṣṇavas».
  DATA_VERSION 3 (tabla `person_tree`). Copia previa: `biodata antes de parampara.db`.
  La v0.1.0 ya no abre esta base de datos (formato 3): instalar la 0.2.0.
  Release: https://github.com/yogeshdasa-droid/Bio-DataBase/releases/tag/v0.2.0

- **25-10-2026 — v0.1.0**: primera versión publicada. Revisión completa de la interfaz;
  estilo «Editor»; texto único dentro de la ficha; atrás/adelante; imagen de ficha
  (DATA_VERSION 2); limpieza de callouts de Obsidian; nombres como enlaces; arreglos de ventana
  estrecha, pestañas y filtro. Release: https://github.com/yogeshdasa-droid/Bio-DataBase/releases/tag/v0.1.0
