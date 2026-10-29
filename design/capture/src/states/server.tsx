import type { StateDef } from "../harness/types"

/**
 * The server's admin page and its email approval page are written by the
 * Rust server itself (bins/server/src/admin.rs and pages.rs build the HTML
 * with `format!` per request); there is no template to render without the
 * server, so the harness holds no state for them. README.md says so.
 */
export const SERVER_STATES: StateDef[] = []
