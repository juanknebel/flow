# Plan: Integración de `flow` con un swarm de agentes local

## Objetivo

Permitir que múltiples agentes locales usen `flow` como tablero compartido de forma robusta,
sin servidor remoto, con coordinación local, auditoría y notificaciones.

## Principios

- **Local-first**: todo corre en la misma máquina.
- **Un solo writer real de SQLite**: evitar contención y corrupción.
- **Interfaz estable para agentes**: JSON/MCP, no parsing de texto.
- **Eventos por cambios**, no polling agresivo.
- **Auditoría completa**: saber qué agente hizo qué y cuándo.
- **Compatibilidad**: no romper la TUI/CLI actuales ni las DBs existentes.

---

## Fase 0 — Hardening base (sin swarm)

Objetivo: dejar `flow-core` preparado para concurrencia y uso por máquinas.

### Tareas

- [ ] Cambiar IDs de cards de millis a **ULID/UUID**. (Descartado: se decidió secuencial `PREFIX-N` por tenant con contador monótono en `projects.next_card_seq`; ver `create_card` en `store_db.rs`.)
- [ ] Agregar `version` / `updated_at` a `Card`.
- [ ] Agregar campos de auditoría: `created_by`, `updated_by`.
- [ ] Implementar migraciones de esquema versionadas.
- [ ] Envolver operaciones multi-paso en transacciones.
- [ ] Agregar optimistic locking en `update_card` / `move_card`.
- [ ] Mejorar filtros y paginación en CLI/API:
  - por proyecto
  - por assignee
  - por prioridad
  - por columna
  - por texto
  - `limit` / `offset`
- [ ] Unificar salida JSON como contrato estable.
- [ ] Agregar tests de concurrencia (varios procesos escribiendo).

### Criterio de salida

- `cargo test --workspace` pasa.
- Dos procesos pueden crear/editar cards sin corrupción.
- La DB antigua migra automáticamente.

---

## Fase 1 — Coordinador local (daemon)

Objetivo: centralizar escrituras y notificaciones en un proceso local.

### Diseño

```text
agent A --\
agent B ---+--> flow daemon (Unix socket / localhost) --> SQLite
agent C --/
```

El daemon:

- es el único writer real de SQLite,
- mantiene cache y locks en memoria,
- serializa operaciones por card/proyecto,
- publica eventos de cambios.

### Tareas

- [ ] Crear `flow daemon` o subcomando `flow serve`.
- [ ] Socket Unix por defecto, localhost opcional.
- [ ] Protocolo JSON-RPC o HTTP local.
- [ ] Cola de operaciones por proyecto.
- [ ] Locks por card (`claim` / `release`).
- [ ] Eventos de cambio (`card.created`, `card.moved`, `card.updated`, `card.deleted`).
- [ ] Cliente en CLI: si el daemon está activo, usarlo; si no, fallback directo.
- [ ] Configuración:
  - `FLOW_DAEMON=1`
  - `FLOW_DAEMON_SOCKET=/path`
  - auto-start opcional.

### Criterio de salida

- 10 agentes concurrentes pueden operar sin `database is locked`.
- Los agentes reciben eventos de cambios en tiempo real.
- La TUI sigue funcionando.

---

## Fase 2 — Interfaz para agentes

Objetivo: exponer `flow` como herramienta nativa para agentes.

### Opción A: MCP server local

Exponer tools:

- `list_cards`
- `get_card`
- `create_card`
- `update_card`
- `move_card`
- `delete_card`
- `list_projects`
- `watch_changes`

### Opción B: SDK/cliente

- Cliente Rust en `flow-core`.
- Cliente Python/Node opcional para agentes externos.
- CLI con `--json` estable.

### Tareas

- [ ] Implementar MCP server local (stdio o socket).
- [ ] Documentar tools y schemas.
- [ ] Agregar ejemplos de uso por agentes.
- [ ] Agregar modo `flow mcp`.

### Criterio de salida

- Un agente LLM puede usar `flow` como tool sin parsear CLI.
- Los cambios se reflejan en TUI/CLI.

---

## Fase 3 — Semántica de swarm

Objetivo: modelar trabajo multi-agente.

### Tareas

- [ ] Estado de card más rico:
  - `claimed`
  - `in_progress`
  - `blocked`
  - `review`
  - `done`
- [ ] `assignee` puede ser agente o humano.
- [ ] Lease/lock con expiración para claims.
- [ ] Idempotency keys en create/update.
- [ ] Dependencias cross-project.
- [ ] Dashboard de swarm:
  - cards por agente
  - bloqueos
  - trabajo en progreso
- [ ] Reintentos y recuperación ante crash de agente.

### Criterio de salida

- Un agente puede reclamar una tarea y liberarla si falla.
- No hay doble asignación silenciosa.

---

## Fase 4 — Observabilidad y seguridad local

### Tareas

- [ ] Tracing estructurado con `agent_id`, `request_id`, `project`.
- [ ] Métricas locales (operaciones, latencia, errores).
- [ ] Token local para daemon (archivo con permisos 600).
- [ ] Rate limiting por agente.
- [ ] Logs rotativos.
- [ ] Health check local.

### Criterio de salida

- Se puede diagnosticar un swarm bloqueado.
- El daemon no acepta conexiones no autorizadas.

---

## Migración

- La DB actual sigue funcionando.
- Las nuevas columnas se agregan con migraciones.
- El daemon es opcional: si no está activo, CLI/TUI usan acceso directo.
- La TUI no pierde funcionalidad.

---

## Riesgos

- SQLite con muchos writers puede seguir siendo cuello de botella si no se usa daemon.
- El daemon agrega complejidad operativa.
- La sincronización de eventos puede generar carga si no se filtra bien.
- La compatibilidad con Jira queda como backend read-only, igual que hoy.

---

## Orden recomendado

1. Fase 0
2. Fase 1
3. Fase 2
4. Fase 3
5. Fase 4

## Primera implementación sugerida

Empezar por:

1. IDs ULID.
2. Auditoría mínima.
3. Migraciones.
4. Daemon local mínimo con JSON-RPC.
5. MCP server básico.

Esto da una base usable para swarm sin romper lo existente.
