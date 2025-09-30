<h1 align = "Center">
<img width = "36" altura = "36" src = "activos/icon.png"/>
Shinkai
</h1>
<div align = "centro">

> Crear poderosos agentes de IA usando AIS locales o remotos

</div>

<p align = "centro">
<img src = "https://img.shields.io/badge/windows-compatible-success?logo=windows&logoColor=white" alt = "Windows Compatible">
<img src = "https://img.shields.io/badge/linux-compatible-success?logo=linux&logoColor=white" alt = "Linux Compatible">
<img src = "https://img.shields.io/badge/macos-apple silicon compatible-success? logotipo = Apple & Logocolor = White" Alt = "MacOS Apple Silicon Compatible">
</p>

<p align = "centro">
<a href = "https://github.com/dcspark/shinkai-apps/stargazers"> <img src = "https://img.shields.io/github/stars/dcspark/shinkaipps?style=social" Alt = "Github Stars"> </a>
<a href = "https://discord.gg/eua45u3seu"> <img src = "https://img.shields.io/discord/130374922084234040412?color=7289da&label=discord&logo=discord&logord&logocord&logocord&logocord&logocord&logoCord&logoCord&logoCord&logocolor=white"Alt = "Discord"> </a>
<a href = "https://x.com/shinkailocalai"> <img src = "https://img.shields.io/twitter/follow/shinkailocalai?style=Social" alt = "Twitter sigue"> </a>
</p>

<p align = "centro">
<strong> construir agentes de IA colaborativos que trabajen juntos, manejen pagos y automatice flujos de trabajo complejos </strong>
<br/>
Shinkai es una plataforma gratuita de código abierto que democratiza la creación de agentes de IA.
No se requiere codificación: solo arrastre, suelte e implementa agentes inteligentes que puedan trabajar en plataformas y manejar tareas del mundo real.
</p>

<p align = "centro">
Lea esto en:
<a href = "readme.zh.md"> chino simplificado </a> |
<a href = "readme.zh-hk.md"> cantonés </a> |
<A href = "readme.ko.md"> coreano </a> |
<a href = "readme.ja.md"> japonés </a> |
<a href = "readme.es.md"> Español </a>
</p>

<p align = "centro">
<a href = "#-características"> características </a> •
<a href = "#-demo"> demo </a> •
<a href = "#-ejemplos"> ejemplos </a> •
<a href = "#-Quick-start"> inicio rápido </a> •
<a href = "#-desarrollo"> desarrollo </a> •
<a href = "https://docs.shinkai.com"> Documentación </a>
</p>

---

## 🚀 Características

** 🎯 🎯 Corriente de agentes sin código **: cree agentes de IA especializados en minutos a través de una interfaz visual intuitiva.No se necesita experiencia de programación.

** 🤖 Orquestación de múltiples agentes **: implementa equipos de agentes que colaboran, comparten contexto y coordinan flujos de trabajo múltiples complejos automáticamente.

** 💰 Arquitectura cripto-nativa **-Soporte incorporado para pagos descentralizados, interacciones defi y agentes económicos autónomos que pueden realizar transacciones de forma independiente.

** 🔗 Soporte de protocolo universal ** - Se integra perfectamente con el Protocolo de contexto del modelo (MCP), lo que hace que sus agentes compatibles con Claude, el cursor y el ecosistema de IA más amplio.

** ⚡ Implementación híbrida ** - Ejecute todo localmente para obtener la máxima privacidad, conectarse a modelos en la nube para capacidades mejoradas o combinar ambos enfoques según sea necesario.

** 🔐 Diseño de seguridad **: sus claves criptográficas, datos confidenciales y cálculos permanecen bajo su control con la arquitectura local primero.

** 🌐 Compatibilidad multiplataforma **-funciona en Windows, MacOS y Linux con rendimiento constante y experiencia en el usuario.

## 🎬 demostración

/

_ Los agentes de Shinkai de observación colaboran para analizar los datos del mercado, ejecutar operaciones y administrar flujos de trabajo complejos de forma autónoma.

## 📋 Ejemplos

** 💹 Bot de comercio autónomo **: implementa un agente que monitorea el sentimiento social, analiza las tendencias del mercado y ejecuta operaciones basadas en estrategias predefinidas.

** 📧 Asistente de correo electrónico inteligente **: cree un agente que clasifique los correos electrónicos entrantes, redacte las respuestas contextuales y programen automáticamente los seguimientos.

** 📊 Agente de inteligencia de datos **: construya agentes que raspe los datos web, realicen análisis de tendencias y generen informes integrales con información procesable.

** Hub de automatización de flujo de trabajo ** - Orquestan múltiples agentes especializados que manejan diferentes componentes de procesos comerciales complejos sin problemas.

** 🏦 Defi Portfolio Manager ** - Configurar agentes que monitorean sus inversiones criptográficas, carteras de reequilibrio y ejecutan estrategias de agricultura de rendimiento.

## 🚀 Inicio rápido

### Instalación de un solo clic

1. ** Descargar ** La última versión para su plataforma desde nuestra [página de comunicados] (https://github.com/dcspark/shinkai-apps/releases)
2. ** Instalar ** y lanzar Shinkai
3. ** Crear ** Su primer agente de IA usando nuestra interfaz guiada paso a paso

### Requisitos del sistema

- ** Memoria **: Mínimo de 4 GB de RAM (recomendado 8GB)
- ** Almacenamiento **: espacio libre de 2 GB
- ** OS **: Windows 10+, MacOS 10.15+ o Linux (Ubuntu 20.04+)

---

## 🛠 Desarrollo

### Descripción general de la arquitectura

Shinkai se construye como un monoreso moderno que usa ** nx ** para la orquestación y la gestión de dependencias.La arquitectura consiste en:

** Aplicaciones centrales: **

-** Shinkai-desktop **-Aplicación Tauri de plataforma cruzada con React Frontend

** Bibliotecas compartidas: **

-** Shinkai-Message-TS **-Protocolos de mensajes y comunicación de red con Shinkai Node
-** Shinkai-Node-State **-Gestión de estado basada en consultas de reacción para datos de nodo
- ** Shinkai-ui **- Componentes reactivos reutilizables con sistema de diseño
- ** Shinkai-Artifacts **- Primitivas de usuario de estilo construidas en Radix y CSS de viento de cola
- ** Shinkai-i18n **- Utilidades de internacionalización impulsadas por i18Next

** Pila de tecnología: **

- ** Frontend **: React 18, mecanografiado, Tailwind CSS, Radix UI
- ** Desktop **: Tauri (Rust + React)
- ** Gestión de estado **: Zustand (UI State) + React Consulter (Estado del servidor)
- ** Sistema de construcción **: VITE, NX MONOREPO
- ** Pruebas **: Biblioteca de pruebas de reacción de Vitest, React

### 🚀 Comenzando

#### 1. Clon y configuración

```bash
git clone https://github.com/dcSpark/shinkai-apps
cd shinkai-apps
nvm use
npm ci
```

#### 2. Descargue los binarios laterales requeridos

Antes de ejecutar Shinkai, deberá descargar el binario de nodo Shinkai incrustado que alimenta la funcionalidad central de la aplicación.Esto se puede hacer con un solo comando basado en su plataforma:

** MacOS (Silicón de Apple): **

```bash
ARCH="aarch64-apple-darwin" \
SHINKAI_NODE_VERSION="v1.1.13" \
OLLAMA_VERSION="v0.12.3" \
npx ts-node ./ci-scripts/download-side-binaries.ts
```

** Linux: **

```bash
ARCH="x86_64-unknown-linux-gnu" \
OLLAMA_VERSION="v0.12.3" \
SHINKAI_NODE_VERSION="v1.1.13" \
npx ts-node ./ci-scripts/download-side-binaries.ts
```

** Windows: **

```powershell
$ENV:OLLAMA_VERSION="v0.12.3"
$ENV:SHINKAI_NODE_VERSION="v1.1.13"
$ENV:ARCH="x86_64-pc-windows-msvc"
npx ts-node ./ci-scripts/download-side-binaries.ts
```

### 📦 Comandos esenciales

#### Servidor de desarrollo

```bash
# Run desktop app (recommended for development)
npx nx serve:tauri shinkai-desktop
```

#### Edificio

```bash
# Build desktop application
npx nx build shinkai-desktop

# Create development build
NODE_OPTIONS="--max_old_space_size=8192" npx nx build shinkai-desktop --config="./src-tauri/tauri.conf.development.json"

# Build all projects
npx nx run-many --target=build
```

#### Datos de terceros y gestión de repositorio

```bash
# Update the built-in Ollama models repository. This repository contains model definitions, tags and metadata for all supported AI models. The command below regenerates the repository files to ensure compatibility with the latest Ollama version and model updates
npx ts-node ./ci-scripts/generate-ollama-models-repository.ts

# Generate Composio apps repository - This script regenerates the repository of pre-built Composio apps and templates that users can import into Shinkai. It ensures the app catalog stays up-to-date with the latest official releases.
deno run -A ./ci-scripts/composio-repository/main.ts

# Generate translations for all languages (EN, ES, etc.)
# This command uses AI to automatically generate translations for all supported languages based on the primary i18n source file (English).
# It ensures consistent translations across the entire application while maintaining natural language quality.

npx nx run shinkai-i18n:i18n
```

#### Prueba y calidad

```bash
# Run tests
npx nx test [project-name]
npx nx run-many --target=test

# Lint code
npx nx lint [project-name]
npx nx run-many --target=lint
```

### 🏗 Estructura del proyecto

```
shinkai-apps/
├── apps/
│   └── shinkai-desktop/          # Main desktop application
├── libs/
│   ├── shinkai-message-ts/       # Core messaging protocol
│   ├── shinkai-node-state/       # State management
│   ├── shinkai-ui/               # Component library
│   ├── shinkai-artifacts/        # UI primitives
│   └── shinkai-i18n/             # Internationalization
├── ci-scripts/                   # Build and deployment scripts
└── tools/                        # Development utilities
```

### 🎨 Directrices de desarrollo de la interfaz de usuario

** Bibliotecas de componentes: **

- ** Radix UI ** - Primitivas de componentes sin estilo y accesibles
- ** CSS **- Utilidad de estilo y diseño receptivo
- ** shadcn/ui **- Patrones de componentes previos a la construcción

** Gestión estatal: **

- ** Condición **- Gestión de estado de la interfaz de usuario del lado del cliente
- ** React Consulty ** - Estado del servidor, almacenamiento en caché y sincronización

### 🌍 Internacionalización

<p align = "inicio">
<img src = "https://img.shields.io/badge/english-supported-success?logo=alphabet&logoColor=white" alt = "inglés compatible">
<img src = "https://img.shields.io/badge/español-supported-success?logo=alphabet&logoColor=white" alt = "español compatible">
<img src = "https://img.shields.io/badge/ 中文 -supported-suCess? logo = alfabet & logocolor = white" alt = "chino admitido">
<img src = "https://img.shields.io/badge/ 粵語 -supported-suCess? logo = alfabet & logocolor = white" alt = "soportado en cantonese">
<img src = "https://img.shields.io/badge/ 日本語 -supported-suCess? logo = alfabet & logocolor = white" alt = "japonés soportado">
<img src = "https://img.shields.io/badge/ 한국어 -supported-suCess? logo = alfabet & logocolor = white" alt = "coreano compatible">
<img src = "https://img.shields.io/badge/bahasa indonesia-supported-success? logotipo = alfabet & logocolor = white" alt = "indonesio compatible con">
<img src = "https://img.shields.io/badge/türkçe-supported-success?logo=alphabet&logoColor=white" alt = "admitido turco">
</p>

Shinkai admite múltiples idiomas a través de nuestro sistema i18n:

```bash
# Add new translation keys
# Edit files in libs/shinkai-i18n/locales/

# Generate updated translation types
npx nx run shinkai-i18n:i18n

# Supported languages: en-US, es-ES, zh-CN, zh-HK, ko-KR, ja-JP, id-ID, tr-TR
```

### 🤝 Contribuir

1. ** Hornada ** El repositorio
2. ** Crear ** Una rama de características: `Git Checkout -B Feature/Amazing -Fature`
3. ** Commit ** Sus cambios: `git commit -m 'agregue la función sorprendente' '
4. ** Push ** a la rama: `Feature de origen de push Git/Amazing-Fature`
5. ** Abierto ** Una solicitud de extracción

### 📚 Recursos adicionales

- ** [Documentación oficial] (https://docs.shinkai.com) ** - Guías integrales y referencia de API
- ** [Discord Community] (https://discord.gg/eua45u3seu) ** - Obtenga ayuda y conéctese con otros desarrolladores
- ** [Actualizaciones de Twitter] (https://x.com/shinkailocalai) ** - Últimas noticias y anuncios

---

<p align = "centro">
<strong> construido con ❤️ por la comunidad Shinkai </strong>
<br/>
<a href = "https://github.com/dcspark/shinkai-apps/blob/main/license"> Licencia Apache </a> •
<a href = "https://github.com/dcspark/shinkai-apps/issues"> Informe de informe </a> •
<a href = "https://github.com/dcspark/shinkai-apps/issues"> función de solicitud </a>
</p>