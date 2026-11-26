# Estado legal de One-Side

Revisión del 25-11-2026 (versión 1.0.0). Esto no es asesoramiento jurídico; resume qué exige cada
pieza y cómo lo cumple One-Side.

## 1. Licencia propia
- El código de One-Side (Rust y Swift) es **MIT** (`LICENSE`, `license = "MIT"` en
  `Cargo.toml`). Los autores conservan el copyright; la licencia permite usar,
  modificar y vender el programa, también a ellos mismos.

## 2. Software de terceros incluido en la app
- 396 librerías Rust, todas con licencias permisivas: MIT, Apache-2.0, BSD,
  ISC, Zlib, Unicode-3.0, 0BSD, Unlicense, BSL-1.0 y CDLA-Permissive-2.0, más una
  MPL-2.0 (`option-ext`, sin modificar). `r-efi` ofrece LGPL solo como alternativa:
  usamos MIT/Apache.
- **Obligación:** incluir sus avisos de copyright y licencia. **Cumplido:**
  `legal/generate.py` crea `THIRD_PARTY_NOTICES.txt` a partir de `cargo metadata`
  en cada `bundle.sh`. Se copia dentro de la app (`Contents/Resources`) y se ve en
  **One-Side › Acerca de One-Side › Licencias de terceros**. El script falla si
  alguna librería no tiene licencia.
- MPL-2.0: no modificamos `option-ext`; su código fuente está en crates.io.

## 3. Software que One-Side descarga aparte (no va dentro de la app)
- Mach9 (Zlib), con Wine, DXMT y vkd3d-proton (LGPL-2.1+), FEX y Mesa/KosmicKrisp
  (MIT). Se descarga de la página de versiones de Mach9 en GitHub: quien lo
  distribuye es Mach9, que publica su código fuente. Aun así, One-Side muestra sus
  licencias y dónde está su código fuente.
- winetricks (LGPL-2.1+) y umu-protonfixes (BSD-2-Clause) se descargan de sus
  repositorios oficiales y no se modifican.
- highball-db: sus datos de juegos son CC0-1.0 (dominio público) y su mapa de
  antitrampas deriva de Are We Anti-Cheat Yet? (MIT), cuyo aviso va en
  `legal/runtime/highball-db.txt` y en los avisos de terceros. One-Side no usa sus
  predicciones basadas en ProtonDB (ODbL). One-Side no está afiliado a Highball; la
  app dice de dónde viene cada dato («Datos de highball-db»).
- **Si algún día el motor se mete dentro de la app**, habrá que incluir los textos
  LGPL (ya están en `legal/runtime/`) y una oferta del código fuente de Wine, DXMT y
  vkd3d-proton en la versión exacta que se incluya.

## 4. Licencias de lo que se instala (tiendas, Microsoft, programas)
- Steam, Epic y Ubisoft Connect se instalan **sin su asistente** (modo silencioso),
  y winetricks instala sin preguntar (`-q`) programas y dependencias de Microsoft
  (Visual C++, .NET, fuentes…). Si se hace así, sus licencias no se muestran.
- **Cumplido:** antes de instalar algo sin asistente, One-Side muestra cuyas
  condiciones se aceptan, con enlace oficial, y solo sigue si pulsas «Aceptar e
  instalar». La aceptación se recuerda por tienda o programa (`consents.json`).
  El bloqueo está en el núcleo (`src/legal.rs`), así que también vale para los
  accesos directos del Dock: al abrir un juego, los arreglos cuya licencia no se ha
  aceptado no se instalan. El ayudante de línea de comandos (`one-side --install`,
  `--install-app`, `--winetricks`) tampoco instala nada sin aceptar: se para, dice
  cuyas condiciones son, con su enlace, y solo sigue si el usuario escribe
  `--acepto`. Lo comprueban las pruebas `ffi::tests::settings_are_saved`
  y `legal::tests::owners`.
  - Steam: https://store.steampowered.com/subscriber_agreement/
  - Epic Games: https://legal.epicgames.com/store/eula
  - Ubisoft: https://legal.ubi.com/eula/
  - Microsoft (Visual C++ y otros): https://visualstudio.microsoft.com/license-terms/
  - Programas del catálogo y recetas de Lutris: la licencia de cada programa.
- El resto de tiendas (GOG, EA, Battle.net, Rockstar, Amazon, itch.io) abren su
  propio instalador, que muestra sus condiciones.
- One-Side solo descarga los instaladores de las webs oficiales. No incluye ni
  enlaza juegos sin licencia: los juegos de pago necesitan que el usuario los tenga.

## 5. Marcas
- Los nombres y logotipos de Steam, Epic Games, GOG, EA, Ubisoft, Battle.net,
  Rockstar, Amazon, itch.io, Microsoft y Windows son de sus dueños. One-Side los
  usa solo para decir qué tienda o sistema se instala o es compatible: es un uso
  referencial permitido en la UE (art. 14.1.c de la Directiva (UE) 2015/2436),
  siempre que no sugiera relación comercial. **Cumplido:** «Acerca de» dice que
  One-Side no está afiliado a ninguno ni tiene su aprobación.
- Los dibujos vienen de Simple Icons (CC0). No hemos aceptado las guías de marca de
  cada empresa. Si alguna lo pidiera, se quitaría la marca de agua girada de su logo.
- One-Side no usa el nombre «CrossOver», ni el código o el catálogo propietario de
  CodeWeavers (CrossTie), ni D3DMetal o el Game Porting Toolkit de Apple, cuya
  licencia no permite redistribuirlos.

## 6. Contenido de terceros (portadas)
- Las portadas, fondos y logotipos de los juegos se piden a Steam al momento y se
  guardan en una caché local. Solo los ve el usuario en su biblioteca. No se
  redistribuyen ni se incluyen en la app.

## 6 bis. Servicios web de Steam y Lutris
- **Steam:** One-Side usa la búsqueda de la tienda (`store.steampowered.com/api/storesearch`)
  y las imágenes públicas de su CDN, sin clave de API ni datos de cuentas de Steam. Aun
  así cumple las Condiciones de uso de la Steam Web API
  (https://steamcommunity.com/dev/apiterms), por ser lo más parecido:
  - uso personal del usuario en su propia biblioteca;
  - política de privacidad publicada (`PRIVACY.md`) y datos guardados solo en su Mac;
  - se ofrece «tal cual», sin garantía («Acerca de»);
  - no aparenta afiliación con Valve y dice que los datos vienen de Steam;
  - no pide ni guarda contraseñas de Steam;
  - no sobrecarga Steam: cada juego se busca una sola vez (el resultado, también el
    «no encontrado», se guarda en caché), de una búsqueda en una y con 250 ms entre
    ellas. Queda muy por debajo del límite de 100 000 consultas al día.
- **Lutris:** su web no publica condiciones de uso (las rutas /terms, /legal, /tos y
  /privacy dan 404 a 25-11-2026). One-Side la usa como su propio cliente: búsquedas solo
  cuando el usuario escribe y sin claves. Cita siempre la fuente («Comunidad de
  Lutris», «Instalador de la comunidad de Lutris»).
- Todas las peticiones dicen que vienen de One-Side
  (`User-Agent: One-Side/<versión> (macOS game launcher)`), sin ningún dato personal.

## 7. Cuentas de Epic y GOG
- Se usan con irori (MIT), como Heroic o Legendary. El inicio de sesión se hace en
  la web de la tienda, así que One-Side nunca ve la contraseña. La sesión se guarda
  en el Mac en una carpeta privada (permisos 0700) y «Cerrar sesión» la borra.
- Epic y GOG no aprueban oficialmente clientes de terceros. El uso es el habitual
  de lanzadores libres, pero el usuario sigue sujeto a los términos de cada tienda.

## 8. Privacidad (RGPD)
- Sin servidores, cuentas propias ni analítica. Ver `legal/PRIVACY.md`, que también
  se ve en la app. Las peticiones a internet no llevan datos personales.

## 9. Cifrado (exportación)
- Solo TLS estándar (rustls/ring) para HTTPS: software de mercado masivo, exento
  según la Nota 3 de la Categoría 5, Parte 2, del Reglamento (UE) 2021/821. Por eso
  `Info.plist` lleva `ITSAppUsesNonExemptEncryption = false`.

## 10. Publicar en GitHub
One-Side se publica como **proyecto de código abierto en GitHub**:
- **Qué se sube:** el código (`src/`, `app/`), `LICENSE`, `README.md`, `legal/`
  (avisos, privacidad, este resumen, el generador y los textos de licencia), el
  icono propio (`brand/icon-orbita.svg`) y los scripts de compilación.
  - No se suben (`.gitignore`): `target/`, `dist/`, los borradores de diseño
    (`brand/*.html`, que analizan la interfaz de otra app) ni `legal/PUBLISHER.txt`.
- **Comprobado:** no hay datos personales, rutas del Mac del autor, claves ni
  contraseñas en el código. Tampoco hay código copiado de CrossOver ni de otros
  proyectos LGPL o GPL: el único código ajeno son las librerías de `Cargo.toml`,
  que se descargan al compilar. Los comentarios no comparan One-Side con productos
  de terceros.
- **Contribuciones:** el README dice que se aceptan con la misma licencia MIT
  (*inbound = outbound*).
- **Identidad del publicador (LSSI):** un repositorio gratuito, sin publicidad ni
  actividad económica, no es un «servicio de la sociedad de la información» de la
  LSSI. Quien lo publica es la cuenta de GitHub que lo sube, y GitHub presta el
  servicio. Si algún día hay ingresos (donaciones con contraprestación, ventas,
  publicidad) o una web propia de descarga, hay que rellenar `legal/PUBLISHER.txt`
  con quién lo publica.
- **Versiones compiladas en «Releases»:**
  - Se pueden subir sin notarizar; el README explica el aviso de macOS. Notarizar no
    es una obligación legal.
  - Para notarizarlas, `./publicar.sh`. Exige `legal/PUBLISHER.txt`, un Developer ID
    y un perfil de `notarytool`, y no sigue si falta algo. Firma con hardened runtime
    y sello de tiempo, notariza, grapa el ticket y lo comprueba con `spctl`.
  - Toda versión compilada lleva dentro `LICENSE`, `THIRD_PARTY_NOTICES.txt`,
    `PRIVACY.md` y `LEGAL.md`, como piden las licencias de las librerías incluidas.
- **Normas de GitHub:** el proyecto no aloja juegos ni software sin licencia, ni
  herramientas para saltarse protecciones anticopia. Cumple sus Acceptable Use
  Policies.

## 11. Lo que no puede cubrir este repaso
- No es asesoramiento jurídico: antes de publicar con fines comerciales conviene que
  un abogado lo revise.
- Lutris no publica condiciones de uso. Su `robots.txt` excluye `/api` para robots
  que rastrean la web. One-Side no rastrea: solo busca cuando el usuario escribe,
  una vez por búsqueda (600 ms después de dejar de escribir), sin repetir (caché) y
  diciendo quién es. Si Lutris pidiera dejar de usarla, se quita la búsqueda en Lutris
  y el resto de la app sigue igual.

- MetalSharp Wine (AGPL-3.0-or-later) es el motor D3DMetal. One-Side lo descarga de sus
  versiones en GitHub, comprueba su suma SHA-256 y su firma, y lo ejecuta sin cambios; no
  lo incluye ni lo modifica. Lleva D3DMetal de Apple (Game Porting Toolkit) sin cambios y
  bajo la licencia de Apple, que está dentro de la app. Detalles en
  `legal/runtime/metalsharp-wine.txt`. One-Side no está afiliado a Apple ni a MetalSharp.

- fsr4-native-macos (MIT) pone AMD FSR 4 donde D3DMetal usa MetalFX para DLSS. One-Side
  lo descarga de sus versiones en GitHub al activarlo, comprueba su suma SHA-256 y lo
  compila en el Mac del usuario, cambiando solo cómo carga sus shaders Metal (para no
  necesitar Xcode); no lo incluye. Su modelo FSR 4 es de AMD, con la licencia de AMD.
  Detalles en `legal/runtime/fsr4-native-macos.txt`. One-Side no está afiliado a AMD.
- OptiScaler (GPL-3.0) se descarga de sus versiones en GitHub solo si el usuario lo activa
  en un juego, se comprueba su suma SHA-256 y se copia sin cambios en la carpeta del juego;
  One-Side no lo incluye y lo quita al desactivarlo. Detalles en `legal/runtime/optiscaler.txt`.

