"""Loopback-only WSL relay for Windows' reserved 55432 port. No logging/data storage."""
import socket
import selectors
import threading

def relay(client):
    with client, socket.create_connection(('127.0.0.1', 55432), timeout=10) as target:
        target.settimeout(None)
        with selectors.DefaultSelector() as selector:
            selector.register(client, selectors.EVENT_READ, target)
            selector.register(target, selectors.EVENT_READ, client)
            while True:
                for key, _ in selector.select():
                    data = key.fileobj.recv(65536)
                    if not data:
                        return
                    key.data.sendall(data)

with socket.socket() as listener:
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(('127.0.0.1', 41432))
    listener.listen(32)
    print('PostgreSQL lab relay: 127.0.0.1:41432', flush=True)
    while True:
        client, _ = listener.accept()
        threading.Thread(target=relay, args=(client,), daemon=True).start()
