import net from "node:net";
import { once } from "node:events";
async function read(socket, count) {
  for (;;) {
    const value = socket.read(count);
    if (value) return value;
    await once(socket, "readable");
  }
}
export async function proxy(kind) {
  const sockets = new Set();
  const server = net.createServer((socket) => {
    sockets.add(socket);
    socket.on("error", () => {});
    socket.on("close", () => sockets.delete(socket));
    void (async () => {
      if (kind === "http") {
        let header = "";
        while (!header.endsWith("\r\n\r\n")) {
          if (header.length > 16000) throw new Error("Header limit");
          header += (await read(socket, 1)).toString();
        }
        if (
          !header.includes(
            "Proxy-Authorization: Basic " +
              Buffer.from("test:proxy-pass").toString("base64"),
          )
        )
          throw new Error("Expected proxy authentication");
        socket.write("HTTP/1.1 200 Connection established\r\n\r\n");
      } else {
        const greeting = await read(socket, 3);
        if (!greeting.equals(Buffer.from([5, 1, 2])))
          throw new Error("SOCKS negotiation");
        socket.write(Buffer.from([5, 2]));
        const auth = await read(socket, 2);
        const user = (await read(socket, auth[1])).toString();
        const size = (await read(socket, 1))[0];
        const pass = (await read(socket, size)).toString();
        if (user !== "test" || pass !== "proxy-pass")
          throw new Error("Expected SOCKS authentication");
        socket.write(Buffer.from([1, 0]));
        const head = await read(socket, 5);
        await read(socket, head[4] + 2);
        socket.write(Buffer.from([5, 0, 0, 1, 127, 0, 0, 1, 0, 0]));
      }
      const target = net.connect(22222, "127.0.0.1");
      sockets.add(target);
      target.on("error", () => socket.destroy());
      target.on("close", () => sockets.delete(target));
      await once(target, "connect");
      socket.pipe(target);
      target.pipe(socket);
    })().catch(() => socket.destroy());
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return {
    port: server.address().port,
    close: () => {
      for (const socket of sockets) socket.destroy();
      server.close();
    },
  };
}
export async function banner(port, socks = false) {
  const socket = net.connect(port, "127.0.0.1");
  socket.on("error", () => {});
  await once(socket, "connect");
  try {
    if (socks) {
      socket.write(Buffer.from([5, 1, 0]));
      await read(socket, 2);
      socket.write(Buffer.from([5, 1, 0, 1, 127, 0, 0, 1, 86, 206]));
      await read(socket, 10);
    }
    let text = "";
    while (!text.includes("\n")) text += (await read(socket, 1)).toString();
    return text;
  } finally {
    socket.destroy();
  }
}
