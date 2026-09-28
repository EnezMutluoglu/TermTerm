// Keep only the newest pending PTY size while an IPC request is in flight.
export function terminalResizeQueue(
  send: (size: { cols: number; rows: number }) => Promise<unknown>,
  onError: (error: unknown) => void,
) {
  type Size = { cols: number; rows: number };
  let pending: Size | undefined, last: Size | undefined;
  let running = false,
    disposed = false;
  const equal = (a: Size | undefined, b: Size) =>
    a?.cols === b.cols && a?.rows === b.rows;
  async function flush() {
    if (running || disposed) return;
    running = true;
    try {
      while (pending && !disposed) {
        const size = pending;
        pending = undefined;
        try {
          await send(size);
          last = size;
        } catch (error) {
          last = undefined;
          if (!disposed) onError(error);
        }
      }
    } finally {
      running = false;
    }
  }
  return {
    request(size: Size, force = false) {
      if (disposed || size.cols < 1 || size.rows < 1) return;
      if (!force && !running && equal(last, size)) return;
      pending = size;
      void flush();
    },
    dispose() {
      disposed = true;
      pending = undefined;
    },
  };
}
