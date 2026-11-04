"""Loopback-only TCPS relay for the existing disposable Oracle container."""
import socket, selectors, threading, subprocess
target_host = subprocess.check_output(['docker','inspect','-f','{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}','termterm-db-oracle'], text=True).strip()
def relay(client):
    try:
        with client, socket.create_connection((target_host,1522),timeout=10) as target:
            target.settimeout(None)
            with selectors.DefaultSelector() as selector:
                selector.register(client,selectors.EVENT_READ,target)
                selector.register(target,selectors.EVENT_READ,client)
                while True:
                    for key,_ in selector.select():
                        data=key.fileobj.recv(65536)
                        if not data:return
                        key.data.sendall(data)
    except OSError:
        client.close()
with socket.socket() as listener:
    listener.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    listener.bind(('127.0.0.1',41522));listener.listen(32)
    print('Oracle TCPS relay: localhost:41522',flush=True)
    while True:
        client,_=listener.accept()
        threading.Thread(target=relay,args=(client,),daemon=True).start()
