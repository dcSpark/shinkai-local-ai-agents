# Repository Guidelines

## Project Structure & Module Organization
- apps: Desktop app at `apps/shinkai-desktop` (React + Vite + Tauri). Source in `src/`, static in `public/`, Rust in `src-tauri/`.
- libs: Shared packages (e.g., `libs/shinkai-ui`, `libs/shinkai-i18n`, `libs/shinkai-message-ts`).
- assets, docs, ci-scripts, tools: Shared resources and automation.
- Tests: Co-located using `__tests__/` and `*.test|spec.(ts|tsx)`.

## Build, Test, and Development Commands
- Dev (desktop): `npx nx serve shinkai-desktop` (Tauri dev; uses `src-tauri/tauri.conf.local.json`).
- Build (desktop): `npx nx build shinkai-desktop` (Tauri build).
- Unit tests: `npx nx test shinkai-desktop` (Vitest; reports to `coverage/apps/shinkai-desktop`).
- Lint: `npx nx lint shinkai-desktop` (ESLint config at repo root and app-level).
- Type check: `npx nx run shinkai-desktop:typecheck`.
- Rust check (tauri): `npx nx run shinkai-desktop:cargo-check`.
- Alternative (inside app): `cd apps/shinkai-desktop && npm run dev|build|lint`.

## Coding Style & Naming Conventions
- Indentation: 2 spaces (`.editorconfig`).
- Formatting: Prettier with Tailwind plugin; single quotes enforced. Run: `npx prettier --write .`.
- Linting: ESLint with import ordering, React Hooks rules, Vitest rules (no focused tests).
- Naming: files/dirs kebab-case (`vector-search/`), React components/types PascalCase, variables/functions camelCase.

## Testing Guidelines
- Framework: Vitest + @testing-library/react.
- Locations: co-locate next to source (e.g., `component/__tests__/x.test.tsx`).
- Conventions: use `*.test.ts(x)`; avoid `only`/focused tests (linted).
- Run locally: `npx nx test shinkai-desktop`.

## Commit & Pull Request Guidelines
- Commits: Prefer Conventional Commits (e.g., `feat:`, `fix:`, `chore:`, `docs:`). Example: `feat: reasoning streaming UI and markdown support`.
- PRs must include:
  - Clear description and linked issues (e.g., `Closes #123`).
  - Screenshots/GIFs for UI changes.
  - Scope-limited diff; updated docs/i18n when relevant.
  - Local verification: lint, unit tests, and `cargo-check` pass.

## Security & Configuration Tips
- Env: copy `apps/shinkai-desktop/.env.example` as needed; never commit secrets.
- Tooling: Node 20+, npm 10+; Rust toolchain for Tauri.
- Patches: `postinstall` runs `patch-package`; update `patches/` when adjusting third-party deps.
