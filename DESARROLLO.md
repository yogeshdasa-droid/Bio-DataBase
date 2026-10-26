# Gaudiya Vaisnavas DB — guía para continuar el proyecto

Documento para retomar el trabajo en el futuro (una persona o una sesión nueva de Claude).
Resume qué es la app, cómo está hecha, qué reglas no hay que romper y qué queda pendiente.
Última actualización: **26-10-2026** (versión **0.3.1**).

> **Para retomar con Claude**, basta con abrir esta carpeta y decir algo como:
> «Lee `DESARROLLO.md` y continuamos con Gaudiya Vaisnavas DB: quiero cambiar…».

---

## 1. Qué es

Aplicación de escritorio para **Windows** con **biografías vaiṣṇavas**: una ficha por cada santo
o devoto, con todos los textos sobre esa persona y búsqueda instantánea.

- Hecha con **Tauri 2** (ventana WebView2), **Rust** (SQLite + búsqueda FTS5) y **React + TypeScript**.
- Repositorio: https://github.com/yogeshdasa-droid/Gaudiya-Vaisnavas-DB (público). Rama `main`.
- Hermana de **GaudiyaDB** (`G:\Whispers\W-DataBase`), de la que se copió la base del código,
  pero es un **proyecto independiente**: no se toca nada de esa carpeta desde aquí, y las dos
  apps pueden estar abiertas a la vez.

### La idea clave: la base de datos ES la biblioteca

Al importar, el texto, las portadas y los PDF / Word originales se copian **dentro** de la base
de datos. Después las carpetas de origen no se vuelven a leer ni se modifican: todo se edita en
la app. Un solo archivo lo contiene todo (y la copia de seguridad es ese archivo).

- Base de datos: `%APPDATA%\com.whispers.biodata\biodata.db` (≈ 240 MB).
- **Copias de seguridad**: automáticas (por defecto cada semana, se guardan las 3 últimas) en
  **`Documentos\Gaudiya Vaisnavas DB\Copias de seguridad\`** («Gaudiya Vaisnavas DB automática
  AAAA-MM-DD.db»), **fuera** de la carpeta de la app: desinstalar con «borrar los datos» no se las lleva
  (la 0.3.0 las ponía en `%APPDATA%\…\copias`, que también se lista). Ajustes › Copias de seguridad:
  hacer una ahora, cambiar la frecuencia, ver la lista, **restaurar** (antes de restaurar se guarda sola
  «… antes de restaurar …») y borrar. Las copias hechas antes de cambios grandes
  («biodata antes de …db», junto a la base de datos) también salen en esa lista.
- La base de datos **no** está en GitHub (es del usuario y es grande).

### Llevar la app a otro PC

El instalador trae **solo el programa, no los datos**. Los datos están en el archivo
`biodata.db`. Para tenerlo todo en otro PC:

1. En este PC: Ajustes › Copias de seguridad › **Guardar una copia en otro sitio…** (un pendrive, por ejemplo).
2. En el otro PC: instalar Gaudiya Vaisnavas DB, abrirlo y en Ajustes › Copias de seguridad › **Restaurar desde un
   archivo…** elegir esa copia.

Si no se restaura nada, la primera vez intenta importar las carpetas de `DEFAULT_SOURCES`
(`G:\Whispers\Data Bases\...`); si no existen, empieza vacía.

---

## 2. Reglas que no hay que romper

1. **Nunca borrar datos del usuario.** Tablas de datos: `persons`, `docs`, `sections`, `files`,
   `bookmarks`, `meta`, `person_photos`, `person_tree` y (formato 4) `history`, `person_tags`,
   `person_images`, `person_distinct`, `calendar_events`. Si cambia su estructura → subir `DATA_VERSION` en
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
cd src-tauri; cargo test   # pruebas automáticas de Rust (22)
npm run app:build      # generar el instalador
```

- El equipo es lento (Kaspersky): la primera compilación completa tarda ~6–10 min;
  después, tras cambiar solo nuestro código, entre segundos y 2 min.
- `npm run app:dev` usa **la misma base de datos real** que la app instalada: no tener las
  dos abiertas a la vez si se van a hacer cambios.
- Importación real a una base de datos de prueba (no toca la buena):
  `$env:BIO_DB = "C:\ruta\test.db"; cargo test real_import -- --ignored --nocapture`
- **Probar la app con una copia** (no toca la buena; imprescindible si se cambia `DATA_VERSION`,
  porque la versión instalada ya no abriría la base de datos): copiar `biodata.db` (+ `-wal`/`-shm`)
  a una carpeta y arrancar con `$env:BIO_DB = "C:\carpeta\biodata.db"; npm run app:dev`
  (`BIO_DB` solo funciona en modo pruebas). Las copias automáticas van entonces a `C:\carpeta\copias`.
- No lanzar `cargo test` mientras `app:dev` está compilando: a veces el enlazador falla
  («unresolved external symbol»); se arregla con `cargo clean -p gaudiya-vaisnavas-db` y volver a compilar.

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
2. `npm run app:build` → `src-tauri/target/release/bundle/nsis/Gaudiya Vaisnavas DB_X.Y.Z_x64-setup.exe`
   (y el programa suelto en `src-tauri/target/release/gaudiya-vaisnavas-db.exe`).
3. Commit + push, y luego la release en GitHub:
   `gh release create vX.Y.Z <instalador> <exe renombrado> --target main --title "Gaudiya Vaisnavas DB X.Y.Z" --notes-file notas.md`

---

## 4. Estructura del código

```
src-tauri/src/                       (Rust: datos y lógica)
  main.rs     comandos que llama la interfaz; protocolo bio:// (portadas, PDF, fotos, imágenes);
              importación inicial (DEFAULT_SOURCES); trabajos en segundo plano; copia automática
              (20 s después de arrancar); limpiezas únicas al arrancar (notas, títulos, claves)
  db.rs       esquema de tablas, DATA_VERSION / FTS_VERSION
  backup.rs   copias de seguridad: automáticas (meta backup_days/keep/last), lista, restaurar
  calendar.rs calendario vaiṣṇava: tithi y mes calculados (Sol y Luna al amanecer en Māyāpur,
              Meeus), lectura de fechas de las fichas («Bhadra Sukla Dvadasi», «8 October 1898»),
              calendario importado (.ics) con nombres → fichas, «hoy y próximos días», años para
              la línea de tiempo
  export.rs   exportar todo a Markdown (Fichas/ + Textos/, enlaces de Obsidian) y una ficha con sus textos
  store.rs    escritura: textos, secciones, fichas (person_key, unir, etiquetas), papelera, fotos
              y galería, versiones anteriores (history), clean_callouts, tidy_doc_titles,
              «Revisar» (textos vacíos, títulos dudosos), fichas distintas, rebuild_fts
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
    Settings.tsx    Ajustes: General (+ calendario, exportar), Copias de seguridad, Revisar,
                    Carpetas de origen, Papelera, Ayuda (+ buscar actualizaciones)
    Calendar.tsx    calendario de Inicio (hoy y próximos 30 días)
    Review.tsx      pestaña Revisar y ayudante de fichas repetidas (dupKey estricto)
    Timeline.tsx, TreeView.tsx   línea de tiempo y árbol dibujado de la Paramparā
    Home.tsx, Results.tsx, ContextMenu.tsx, ui.tsx
  update.ts         ¿hay versión nueva en GitHub? (una vez al día; aviso en la barra de estado)
```

Direcciones internas de archivos (las sirve `serve_file` en `main.rs`):
`http://bio.localhost/cover/<docId>`, `/original/<docId>`, `/photo/<personId>`, `/image/<imageId>`.

---

## 5. Qué hace hoy (v0.3.0)

Novedades de la 0.3.0 (formato de base de datos 4):
- **Copias de seguridad** automáticas y **restaurar** desde Ajustes (ver punto 1).
- **Versiones anteriores** de cada texto y ficha (menú «…» del lector y de la ficha, y botón derecho):
  se guarda cómo estaba antes de cada cambio (una versión cada 10 min como mucho, las 30 últimas);
  volver a una versión también se puede deshacer.
- **Calendario vaiṣṇava** en Inicio: hoy y los próximos 30 días. El usuario sigue un calendario
  Gauḍīya que exporta en **.ics** (`gaudiya_calendar.ics`): se importa en Ajustes › General o
  arrastrándolo a la ventana, y **sus fechas mandan** (Ekādaśī, Pāraṇa, festivales, Āvirbhāva /
  Tirobhāva enlazados con las fichas por el nombre; si hay fichas repetidas, gana la de la Paramparā).
  Lo que no está en él se calcula (marcado ≈): tithi al amanecer en Māyāpur, leído de los campos
  Aparición / Desaparición. Ajustes muestra los nombres del calendario que no tienen ficha.
- **Ficha**: día lunar bajo las fechas; guru y discípulos como enlaces (también «Rupa Gosvami (diksa);…»),
  y si el campo está vacío, los de la Paramparā; **etiquetas** (filtro en la barra lateral);
  **galería de imágenes** (ampliar, usar como principal, quitar); exportar con sus textos; imprimir.
- **Ajustes › Revisar**: ayudante de **fichas repetidas** (57 grupos; uno a uno: unir / son distintas /
  saltar; nunca marca nada solo), **títulos dudosos** (renombrar ahí mismo) y **textos vacíos**.
  Los títulos «ÃTHE…» y en fuente Balaram se arreglan solos al abrir.
- **Línea de tiempo** y **árbol dibujado** de la Paramparā (botones en Inicio).
- Búsqueda: filtros **Solo Guru Paramparā** y **Notas / PDF / Word**.
- **Exportar todo a Markdown** (Ajustes › General): carpetas Fichas/ y Textos/ para Obsidian.
- **Aviso de versión nueva** (GitHub, una vez al día) y botón en Ayuda; abre la descarga del instalador.

Lo de antes:

- **Fichas**: nombre, otros nombres, aparición, desaparición, lugar, guru, discípulos, obras,
  notas y **una imagen** (pinchar en el cuadro grande; se guarda en `person_photos`).
- **Personas en dos grupos**: **Guru Paramparā** (árbol guru → discípulos, plegable) y
  **Otros vaiṣṇavas** (por orden alfabético). Se ordena **arrastrando con el ratón**: encima de una
  ficha = su discípulo; en el borde de arriba / abajo = antes / después; a «Otros vaiṣṇavas» = sacarla
  (sus discípulos suben a su lugar). También en el menú del botón derecho. Tabla `person_tree`
  (DATA_VERSION 3); al unir o borrar fichas el árbol se mantiene. La primera vez se rellena sola con
  la línea habitual desde Śrī Caitanya (`PARAMPARA` en `store.rs`, meta `parampara_seeded`;
  `seed_parampara_once` en `main.rs`, al arrancar y al terminar una importación, solo si ya hay fichas).
  El arrastre está hecho a mano (mousedown/mousemove): el arrastre del navegador no funciona porque
  Tauri lo usa para importar archivos.
- **Estilo de la app** (Ajustes › General): **«VS Code» por defecto** (colores Dark/Light Modern de
  VS Code, letra Segoe UI, esquinas rectas, sin degradados) o «Clásico (anterior)». Es solo CSS:
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

### Rellenar fichas a partir de los textos (cómo se hizo, para repetirlo)

Se hizo el 26-10-2026 con las 33 fichas de la Guru Paramparā. Los textos son prosa libre
(«was born in 1426 Saka…», «disciple of…»), así que no sirven reglas automáticas: **Claude lee
los textos** y escribe los datos. Pasos:

1. **Exportar** (solo lectura): un test temporal `#[ignore]` en Rust abre
   `%APPDATA%\com.whispers.biodata\biodata.db` en modo lectura y escribe, por ficha, un archivo
   `p<id>.txt` con sus textos (primero las notas de Obsidian, luego Goswami Grantha, luego Calibre;
   hasta ~25.000 letras por texto) y un `index.tsv` (id, nombre, padre en el árbol, campos actuales).
   Fichas de la Paramparā: `SELECT … FROM person_tree t JOIN persons p ON p.id = t.person_id`.
2. **Resumir**: un script de Node deja de cada ficha el comienzo (~3.500 letras) + las frases con
   palabras clave (born, appear, disappear, departed, initiat, disciple, guru, father, mother,
   village, wrote, Saka, A.D., tithi, manjari…). Unas 7.000 letras por ficha.
3. **Leer y anotar** en JSON: `{id, name, aka, appearance, disappearance, place, guru, disciples,
   works, notes}`. Criterios: en inglés como los textos; solo lo que dicen los textos (si no
   aparece, vacío); si las fuentes difieren, las dos («1500 (otra fuente: 1503)»); dīkṣā / śikṣā /
   sannyāsa indicados; en notas: padres, hermanos, identidad en Kṛṣṇa-līlā y hechos principales;
   avisar en notas si un texto es de otra persona con el mismo nombre. No poner en «otros
   nombres» palabras genéricas (Avadhuta, Namacarya…): ese campo se usa para unir textos.
4. **Guardar**: otro test temporal hace primero `VACUUM INTO "biodata antes de rellenar fichas.db"`,
   comprueba que el nombre de cada id coincide, **solo rellena campos vacíos** y guarda con
   `store::update_person` (así se actualiza también la búsqueda `persons_fts`).
5. Borrar los tests temporales (`git checkout src-tauri/src/search.rs`) y avisar al usuario de que
   reabra la app (si edita una ficha que tenía abierta podría pisar los datos nuevos).

Para «Otros vaiṣṇavas» (~1.850 fichas, 1.539 con menos de 2.000 letras) el usuario prefirió no
hacerlo de momento por el tiempo y el uso de Claude; si se hace, por tandas y con ayudantes en paralelo.

---

## 7. Pendientes e ideas

- Repasar en Ajustes › Revisar: 57 grupos de fichas repetidas, 10 títulos dudosos, 15 textos vacíos.
- OCR del único PDF escaneado («Krishna dasa babaji - v.1»): no se metió un motor de OCR en la app
  por un solo archivo; se podría transcribir aparte.
- Firma digital del .exe (quita el aviso de Windows): cuesta un certificado anual; no se ha hecho.
- Rellenar las fichas (fechas, guru, discípulos…) a partir de los textos: **hecho para las 33 de la
  Guru Paramparā** (26-10-2026, leyendo Claude los textos; solo campos vacíos, en inglés como los
  textos). Quedan las de «Otros vaiṣṇavas».
- Enlaces de nombres: ahora se calculan en la pantalla cada vez; si algún día va lento con textos
  muy largos, se podría calcular en Rust.

---

## 8. Historial

- **26-10-2026 — v0.3.1**: la app pasa a llamarse **Gaudiya Vaisnavas DB** (antes Bio-Data): ventana,
  instalador, programa (`gaudiya-vaisnavas-db`), repositorio de GitHub (renombrado; los enlaces
  antiguos redirigen). El identificador `com.whispers.biodata` **no cambia**: así la base de datos y
  los ajustes siguen en el mismo sitio. Las copias automáticas pasan a Documentos.
  Motivo: al instalar la 0.3.0 se desinstaló la 0.2.1 con «borrar los datos de la aplicación»; la
  carpeta `%APPDATA%\com.whispers.biodata` quedó vacía y la app reimportó todo desde cero (sin las
  fichas rellenadas). Se recuperó con la copia de prueba de esa mañana; lo reimportado quedó en
  `copias\antes de recuperar 2026-10-26`. **Al desinstalar, no marcar «borrar los datos».**

- **26-10-2026 — v0.3.0**: copias automáticas y restaurar; versiones anteriores;
  calendario vaiṣṇava (calculado + importado de .ics); etiquetas; galería; guru/discípulos enlazados;
  Revisar (repetidas, títulos, vacíos); línea de tiempo; árbol dibujado; filtros Paramparā y tipo;
  exportar a Markdown; aviso de versión nueva. DATA_VERSION 4 (solo tablas nuevas, vacías).
  Probado con una copia de la base de datos (`BIO_DB`); 22 pruebas automáticas. GIF de novedades en
  `docs/novedades-0.3.0.gif` (capturas por CDP + gifenc). Release: https://github.com/yogeshdasa-droid/Gaudiya-Vaisnavas-DB/releases/tag/v0.3.0

- **26-10-2026**: rellenadas a mano (Claude leyó sus textos) las 33 fichas de la Guru Paramparā:
  aparición, desaparición, lugar, guru, discípulos, obras, otros nombres y notas (padres, identidad
  en Kṛṣṇa-līlā…). Solo campos vacíos. Copia previa: `biodata antes de rellenar fichas.db`.

- **26-10-2026 — v0.2.1**: arreglo: en una instalación nueva la Guru Paramparā salía vacía
  (se rellenaba antes de la importación inicial, cuando aún no había fichas).

- **26-10-2026 — v0.2.0**: estilo de la app «VS Code» (por defecto; el anterior sigue en Ajustes).
  Personas en «Guru Paramparā» (árbol, arrastrar y soltar) y «Otros vaiṣṇavas».
  DATA_VERSION 3 (tabla `person_tree`). Copia previa: `biodata antes de parampara.db`.
  La v0.1.0 ya no abre esta base de datos (formato 3): instalar la 0.2.0.
  Release: https://github.com/yogeshdasa-droid/Gaudiya-Vaisnavas-DB/releases/tag/v0.2.0

- **25-10-2026 — v0.1.0**: primera versión publicada. Revisión completa de la interfaz;
  estilo «Editor»; texto único dentro de la ficha; atrás/adelante; imagen de ficha
  (DATA_VERSION 2); limpieza de callouts de Obsidian; nombres como enlaces; arreglos de ventana
  estrecha, pestañas y filtro. Release: https://github.com/yogeshdasa-droid/Gaudiya-Vaisnavas-DB/releases/tag/v0.1.0
