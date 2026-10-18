// The desktop side of the extension suite: the real endpoint and core over
// a throwaway vault (`apps/desktop/src-tauri/examples/browser-endpoint.rs`)
// and the real native host, both built by `pnpm run build:native`.
import { execFileSync, spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

export interface Event {
  event: string;
  [k: string]: unknown;
}

/** Where cargo puts its builds (honours CARGO_TARGET_DIR). */
export function targetDir(): string {
  const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { encoding: "utf8", maxBuffer: 64 << 20 }));
  return meta.target_directory as string;
}

export class Desktop {
  private queue: Event[] = [];
  private waiters: { want: string; resolve(e: Event): void }[] = [];
  readonly ready: Promise<Event>;

  private constructor(private proc: ChildProcessWithoutNullStreams) {
    const lines = createInterface({ input: proc.stdout });
    lines.on("line", (line) => {
      let e: Event;
      try {
        e = JSON.parse(line);
      } catch {
        return;
      }
      const i = this.waiters.findIndex((w) => w.want === e.event);
      if (i >= 0) this.waiters.splice(i, 1)[0].resolve(e);
      else this.queue.push(e);
    });
    proc.stderr.on("data", (d) => process.stderr.write(d));
    this.ready = this.next("ready");
  }

  static start(scratch: string, profile: string, extra: string[] = []): Desktop {
    // LOCKDOWN_TEST_BINS: a folder holding private copies of both, for a
    // machine where several checkouts share one cargo target directory.
    const bins = process.env.LOCKDOWN_TEST_BINS;
    const target = bins ? "" : targetDir();
    const fixture = bins ? join(bins, "browser-endpoint") : join(target, "debug", "examples", "browser-endpoint");
    const host = bins ? join(bins, "lockdown-native-host") : join(target, "debug", "lockdown-native-host");
    for (const f of [fixture, host]) {
      if (!existsSync(f)) throw new Error(`${f} is missing; run \`pnpm run build:native\` first`);
    }
    const proc = spawn(fixture, ["--dir", scratch, "--host", host, "--profile", profile, ...extra], { stdio: ["pipe", "pipe", "pipe"] });
    return new Desktop(proc);
  }

  /** The next event named `want` (queued ones first). */
  next(want: string, timeoutMs = 15_000): Promise<Event> {
    const i = this.queue.findIndex((e) => e.event === want);
    if (i >= 0) return Promise.resolve(this.queue.splice(i, 1)[0]);
    return new Promise((resolve, reject) => {
      const w = { want, resolve };
      this.waiters.push(w);
      setTimeout(() => {
        const at = this.waiters.indexOf(w);
        if (at >= 0) {
          this.waiters.splice(at, 1);
          reject(new Error(`no '${want}' event from the desktop fixture`));
        }
      }, timeoutMs);
    });
  }

  /** Sends one line and waits for the event it answers with. */
  async ask(line: string, answer: string): Promise<Event> {
    const reply = this.next(answer);
    this.proc.stdin.write(line + "\n");
    return reply;
  }

  stop() {
    this.proc.stdin.end("quit\n");
    setTimeout(() => this.proc.kill(), 2000).unref();
  }
}
