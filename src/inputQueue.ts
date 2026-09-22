/** One in-flight IPC write per terminal preserves byte order, including broadcast. */
export class InputQueue {
  private queues = new Map<
    string,
    { text: string; running: boolean; cancelled: boolean }
  >();
  constructor(private write: (id: string, data: string) => Promise<unknown>) {}
  send(id: string, text: string, onError: (e: unknown) => void) {
    let queue = this.queues.get(id);
    if (!queue) {
      queue = { text: "", running: false, cancelled: false };
      this.queues.set(id, queue);
    }
    if (queue.text.length + text.length > 1024 * 1024) {
      onError(
        new Error(
          "Terminal input queue is full; wait for the connection before pasting more text.",
        ),
      );
      return;
    }
    queue.text += text;
    if (queue.running) return;
    queue.running = true;
    const current = queue;
    void (async () => {
      try {
        while (current.text && !current.cancelled) {
          let end = Math.min(32768, current.text.length);
          if (
            end < current.text.length &&
            /[\uD800-\uDBFF]/.test(current.text[end - 1])
          )
            end--;
          const data = current.text.slice(0, end);
          current.text = current.text.slice(end);
          await this.write(id, data);
        }
      } catch (e) {
        if (!current.cancelled) onError(e);
      } finally {
        if (this.queues.get(id) === current) this.queues.delete(id);
      }
    })();
  }
  cancel(id?: string) {
    for (const [key, queue] of this.queues) {
      if (id && id !== key) continue;
      queue.cancelled = true;
      queue.text = "";
      this.queues.delete(key);
    }
  }
}
