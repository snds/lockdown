import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

import type { CameFrom, DemoOpened, DevtoolsClient, DevtoolsStatus, MenuAction, ResetPlan } from "./devtools-client"

const call = <T,>(cmd: string, args?: Record<string, unknown>) => invoke<T>(`plugin:devtools|${cmd}`, args)

/** The developer commands over Tauri's IPC (`plugin:devtools|…`). */
export const tauriDevtoolsClient: DevtoolsClient = {
  status: () => call<DevtoolsStatus>("status").catch(() => null),
  resetPlan: () => call<ResetPlan>("reset_plan"),
  resetRun: (confirm) => call<void>("reset_run", { confirm }),
  demoOpen: () => call<DemoOpened>("demo_open"),
  demoReset: () => call<DemoOpened>("demo_reset"),
  demoLeave: () => call<CameFrom>("demo_leave"),
  demoLoadPack: () => call<DemoOpened | null>("demo_load_pack"),
  onMenu: (on) => {
    const off = listen<MenuAction>("devtools://menu", (e) => on(e.payload))
    return () => {
      void off.then((f) => f())
    }
  },
}
