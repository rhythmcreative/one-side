# Privacidad en One-Side

**Resumen: One-Side no recoge ni envía datos personales a sus autores. No tiene
cuentas propias, ni servidores, ni analítica, ni publicidad.**

## Qué se guarda y dónde
Todo se queda en tu Mac, en `~/Library/Application Support/One-Side`:
- Las botellas, sus ajustes y los programas que instalas.
- Una caché de imágenes (portadas, iconos y logotipos).
- Si inicias sesión en Epic Games o GOG: la sesión que te dan esas tiendas, en la
  carpeta `irori`, que solo puede leer tu usuario. One-Side nunca ve tu contraseña:
  el inicio de sesión se hace en la web de cada tienda. «Cerrar sesión» la borra.

Puedes borrar todo eliminando esa carpeta.

## A quién se conecta One-Side
Solo cuando hace falta para lo que pides, y sin enviar datos tuyos:

| Servicio | Para qué | Qué recibe |
|---|---|---|
| Steam (store.steampowered.com, steamstatic.com) | Buscar portadas e imágenes; descargar el instalador de Steam | El nombre del juego que se busca |
| GitHub (github.com, githubusercontent.com) | Descargar el motor Mach9, winetricks y la base de arreglos | Nada más que la descarga |
| Lutris (lutris.net) | Buscar instaladores de la comunidad | El texto que buscas |
| Simple Icons (simpleicons.org) | Logotipos de las tiendas | Nada más que la descarga |
| Webs oficiales de cada tienda o programa | Descargar su instalador cuando lo pides | Nada más que la descarga |
| Epic Games y GOG | Solo si inicias sesión: tu biblioteca y las descargas | Lo que esas tiendas requieren, según sus propias políticas de privacidad |

Como cualquier conexión a internet, estos servicios ven tu dirección IP. Cada uno
la trata según su propia política de privacidad.

## Responsable
One-Side es software libre que se ejecuta solo en tu equipo. Sus autores no reciben
ni tratan ningún dato personal tuyo, así que no actúan como responsables del
tratamiento en el sentido del Reglamento General de Protección de Datos (RGPD).
